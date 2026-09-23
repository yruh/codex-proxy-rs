//! 账号额度的只读容量估算，不参与金额结算或账号调度。

use chrono::{DateTime, Duration, Utc};

use super::provider_credentials::{AccountUsagePeriod, ProviderQuota, ProviderQuotaWindow};
use super::quota_forecast_sampling::{QuotaForecastMethod, QuotaForecastSample};

const DAY_SECONDS: u64 = 86_400;
const MIN_USED_PERCENT: f64 = 5.0;
const LOW_SAMPLE_PERCENT: f64 = 10.0;

/// 历史配额只保存观测事实，不把观测间的重置时间伪装成精确时间。
#[derive(Debug, Clone)]
pub struct QuotaHistoryDocument {
    pub observed_at: DateTime<Utc>,
    pub document: super::provider_credentials::ProviderDocument,
    pub snapshot: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountQuotaCycle {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reset_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub used_percent: f64,
    pub boundary: String,
    pub uncertain_after: Option<DateTime<Utc>>,
    pub current: bool,
    pub requests: u64,
    pub tokens: u64,
    pub usd: f64,
    pub priced_usd: f64,
    pub incomplete_cost: bool,
}

/// 同一周窗口的低读数需后续观测确认；单次乱序或百分比抖动不切周期。
#[must_use]
pub fn quota_cycles(
    mut observations: Vec<ProviderQuota>,
    now: DateTime<Utc>,
) -> Vec<AccountQuotaCycle> {
    observations.sort_by_key(|q| q.observed_at);
    let mut cycles: Vec<AccountQuotaCycle> = Vec::new();
    let mut pending_drop: Option<(DateTime<Utc>, f64)> = None;
    for quota in observations {
        let Some(at) = quota.observed_at.filter(|at| *at <= now) else {
            continue;
        };
        let Some((window, _)) = quota
            .usage_windows()
            .find(|(_, period)| *period == AccountUsagePeriod::Weekly)
        else {
            continue;
        };
        let (Some(reset), Some(used)) = (window.reset_at, window.used_percent) else {
            continue;
        };
        if reset <= at || !used.is_finite() || !(0.0..=100.0).contains(&used) {
            continue;
        }
        let nominal_start = reset - Duration::days(7);
        if nominal_start >= at {
            continue;
        }
        let mut start = nominal_start;
        let mut boundary = "scheduled";
        let mut uncertain_after = None;
        if let Some(previous) = cycles.last_mut() {
            if at <= previous.observed_at {
                continue;
            }
            let changed = (reset - previous.reset_at).abs() > Duration::seconds(2);
            if !changed && used + 1.0 < previous.used_percent {
                if let Some((first_low_at, _)) = pending_drop {
                    start = first_low_at;
                    boundary = "observed_reset";
                    uncertain_after = Some(previous.observed_at);
                } else {
                    pending_drop = Some((at, used));
                    continue;
                }
            } else if changed {
                // 正常换周的边界由上游 reset 给出；提前换桶只能定位到两次观测之间。
                if nominal_start < previous.reset_at - Duration::seconds(2) {
                    start = at;
                    boundary = "window_changed";
                    uncertain_after = Some(previous.observed_at);
                } else {
                    start = nominal_start.max(previous.reset_at);
                }
            } else {
                pending_drop = None;
                previous.observed_at = at;
                previous.used_percent = used;
                continue;
            }
            pending_drop = None;
            previous.end = previous.end.min(start);
            previous.current = false;
        }
        if start >= reset || start > now {
            continue;
        }
        cycles.push(AccountQuotaCycle {
            start,
            end: reset.min(now),
            reset_at: reset,
            observed_at: at,
            used_percent: used,
            boundary: boundary.to_owned(),
            uncertain_after,
            current: reset > now,
            requests: 0,
            tokens: 0,
            usd: 0.0,
            priced_usd: 0.0,
            incomplete_cost: false,
        });
    }
    cycles.retain(|cycle| cycle.end > cycle.start);
    cycles
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountQuotaForecastReport {
    pub account_id: String,
    pub generated_at: DateTime<Utc>,
    pub forecasts: [AccountQuotaForecast; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountQuotaForecast {
    pub period: AccountUsagePeriod,
    pub target_seconds: u64,
    pub extrapolated: bool,
    pub source: Option<QuotaForecastSource>,
    pub unavailable_reason: Option<&'static str>,
    pub low_sample: bool,
    pub incomplete_cost: bool,
    pub incomplete_tokens: bool,
    pub estimated_tokens: Option<u64>,
    pub estimated_usd: Option<f64>,
    pub estimated_priced_usd: Option<f64>,
    pub remaining_priced_usd: Option<f64>,
    pub effective_pricing_multiplier: Option<f64>,
    /// 剩余估算始终属于源窗口，不随目标周期折算。
    pub remaining_tokens: Option<u64>,
    pub remaining_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaForecastSource {
    pub label: String,
    pub used_percent: Option<f64>,
    pub observed_at: Option<DateTime<Utc>>,
    pub reset_at: DateTime<Utc>,
    pub tokens: Option<u64>,
    pub usd: Option<f64>,
}

/// 优先预测真实的对应窗口；缺少对应周期时只给出明确标识的 7/30 天容量折算。
/// 本地日志不能证明站外消耗或完整留存，因此即使样本充足也不声称官方额度。
#[must_use]
pub fn account_quota_forecasts(
    quota: &ProviderQuota,
    account_added_at: DateTime<Utc>,
    now: DateTime<Utc>,
    samples: &[QuotaForecastSample],
) -> [AccountQuotaForecast; 2] {
    [AccountUsagePeriod::Weekly, AccountUsagePeriod::Monthly].map(|period| {
        let selected = quota
            .usage_windows()
            .find(|(_, source_period)| *source_period == period)
            .or_else(|| quota.usage_windows().min_by_key(|(_, period)| *period));
        let mut forecast = AccountQuotaForecast {
            period,
            target_seconds: match period {
                AccountUsagePeriod::Weekly => 7 * DAY_SECONDS,
                AccountUsagePeriod::Monthly => 30 * DAY_SECONDS,
            },
            extrapolated: false,
            source: None,
            unavailable_reason: Some("没有可统计的周/月额度窗口，请先刷新账号额度。"),
            low_sample: false,
            incomplete_cost: false,
            incomplete_tokens: false,
            estimated_tokens: None,
            estimated_usd: None,
            estimated_priced_usd: None,
            remaining_priced_usd: None,
            effective_pricing_multiplier: None,
            remaining_tokens: None,
            remaining_usd: None,
        };
        if let Some((window, source_period)) = selected {
            forecast.project(
                window,
                source_period,
                quota.observed_at,
                account_added_at,
                now,
                samples.iter().find(|sample| sample.key == window.key),
            );
        }
        forecast
    })
}

impl AccountQuotaForecast {
    fn project(
        &mut self,
        window: &ProviderQuotaWindow,
        source_period: AccountUsagePeriod,
        observed_at: Option<DateTime<Utc>>,
        account_added_at: DateTime<Utc>,
        now: DateTime<Utc>,
        sample: Option<&QuotaForecastSample>,
    ) {
        let (Some(seconds), Some(reset_at)) = (window.window_seconds, window.reset_at) else {
            return;
        };
        self.extrapolated = source_period != self.period;
        if !self.extrapolated {
            self.target_seconds = seconds;
        }
        let percent = window
            .used_percent
            .filter(|p| p.is_finite() && (0.0..=100.0).contains(p));
        let usage = sample.map(|sample| &sample.cycle_usage);
        let usd = usage
            .filter(|usage| usage.known_cost_count > 0)
            .map(|usage| usage.usd)
            .filter(|value| value.is_finite() && *value >= 0.0);
        let start = i64::try_from(seconds)
            .ok()
            .and_then(Duration::try_seconds)
            .and_then(|duration| reset_at.checked_sub_signed(duration));
        self.source = Some(QuotaForecastSource {
            label: window.label.clone(),
            used_percent: percent,
            observed_at,
            reset_at,
            tokens: usage.map(|usage| usage.tokens),
            usd,
        });
        self.incomplete_cost = usage.is_none_or(|usage| {
            usage.unavailable_cost_count > 0
                || usage.known_cost_count == 0
                || usage.known_cost_count != usage.request_count
        });
        self.incomplete_tokens = usage.is_some_and(|usage| usage.missing_token_count > 0);
        let method = sample.map_or(QuotaForecastMethod::Cumulative, |sample| sample.method);
        let Some(start) = start.filter(|start| *start <= now && now < reset_at) else {
            self.unavailable_reason = Some("额度窗口已过期或边界无效，请刷新账号额度后重试。");
            return;
        };
        if !observed_at.is_some_and(|observed| start <= observed && observed <= now) {
            self.unavailable_reason = Some("缺少本周期的额度快照，请先刷新账号额度。");
            return;
        }
        if sample.is_some_and(|sample| sample.discontinuous) {
            self.unavailable_reason =
                Some("额度观测出现回落或累计记录不连续，正在重新积累配对样本。");
            return;
        }
        if account_added_at > start && method == QuotaForecastMethod::Cumulative {
            self.unavailable_reason = Some(
                "本周期开始时的记录不完整，正在积累至少 5 个百分点的配对观测，无需等待下次重置。",
            );
            return;
        }
        let Some(percent) = percent else {
            self.unavailable_reason = Some("已用比例未知，请刷新额度后查看预测。");
            return;
        };
        let Some(usage) = usage.filter(|usage| usage.request_count > 0) else {
            self.unavailable_reason = Some("本周期没有已采集的本地或代理用量，暂时无法预测额度。");
            return;
        };
        let Some(sample) = sample.filter(|sample| {
            sample.end_at == observed_at.unwrap_or(now)
                && start <= sample.start_at
                && sample.start_at <= sample.end_at
                && sample.sampled_percent.is_finite()
                && sample.sampled_percent >= MIN_USED_PERCENT
                && sample.sampled_percent <= percent
        }) else {
            self.unavailable_reason =
                Some("有效额度进度不足 5 个百分点或采样边界无效，请继续积累用量。");
            return;
        };
        self.low_sample = sample.sampled_percent < LOW_SAMPLE_PERCENT
            || (method == QuotaForecastMethod::Incremental && sample.block_count < 2);
        // 漏记和个别缺失只影响精度，仍按已记录数值估算，不按请求数补齐未知消耗。
        // 预测是近似展示值；不复用为账单金额，也不把月折算当成自然月或额外余额。
        // 已发生的用量保持本周期累计，只把近期消耗比例用于尚未使用的额度。
        let factor = self.target_seconds as f64 / seconds as f64;
        let remaining_factor = (100.0 - percent) / sample.sampled_percent;
        let remaining_tokens = Some(sample.usage.tokens)
            .filter(|tokens| *tokens > 0)
            .and_then(|value| estimate(value as f64, remaining_factor));
        self.remaining_tokens = remaining_tokens.and_then(|value| estimate_tokens(value, 1.0));
        self.estimated_tokens = remaining_tokens
            .and_then(|remaining| estimate_tokens(usage.tokens as f64 + remaining, factor));
        self.remaining_usd = Some(&sample.usage)
            .filter(|usage| usage.known_cost_count > 0 && usage.usd.is_finite() && usage.usd >= 0.0)
            .and_then(|usage| estimate(usage.usd, remaining_factor));
        self.estimated_usd = usd
            .zip(self.remaining_usd)
            .and_then(|(used, remaining)| estimate(used + remaining, factor));
        self.remaining_priced_usd = self
            .remaining_usd
            .and(sample.usage.priced_usd)
            .and_then(|value| estimate(value, remaining_factor));
        self.estimated_priced_usd = usd
            .and(usage.priced_usd)
            .zip(self.remaining_priced_usd)
            .and_then(|(used, remaining)| estimate(used + remaining, factor));
        self.effective_pricing_multiplier = self
            .estimated_priced_usd
            .zip(self.estimated_usd)
            .and_then(|(priced, raw)| {
                (raw > 0.0)
                    .then_some(priced / raw)
                    .filter(|value| value.is_finite())
            });
        self.unavailable_reason = if self.estimated_tokens.is_none() && self.estimated_usd.is_none()
        {
            Some("本周期暂无可用于估算的 Token 或费用数据，请积累用量后重试。")
        } else {
            None
        };
    }
}

fn estimate(value: f64, factor: f64) -> Option<f64> {
    let estimate = value * factor;
    (estimate.is_finite() && estimate >= 0.0).then_some(estimate)
}

fn estimate_tokens(value: f64, factor: f64) -> Option<u64> {
    estimate(value, factor)
        .filter(|value| value.round() < u64::MAX as f64)
        .map(|value| value.round() as u64)
}
