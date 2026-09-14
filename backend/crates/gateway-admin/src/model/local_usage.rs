//! 本地直连用量的同步事实，不参与客户端密钥扣费。

use chrono::{DateTime, Utc};

use super::AdminError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalUsageRecord {
    pub record_id: String,
    pub revision: i64,
    pub occurred_at: DateTime<Utc>,
    pub session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub transport: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub excluded: bool,
    pub estimated_usd: Option<String>,
}

impl LocalUsageRecord {
    /// 只接受有界统计字段；缺失的缓存和计时保持缺失，不补成零。
    ///
    /// # Errors
    ///
    /// 标识、修订号、token 子集或计时非法时返回校验错误。
    pub fn validate(&self) -> Result<(), AdminError> {
        if let Some(amount) = &self.estimated_usd {
            let mut parts = amount.split('.');
            let whole = parts.next().unwrap_or("");
            let fraction = parts.next();
            if whole.is_empty()
                || whole.len() > 12
                || !whole.bytes().all(|b| b.is_ascii_digit())
                || parts.next().is_some()
                || fraction.is_some_and(|f| {
                    f.is_empty() || f.len() > 12 || !f.bytes().all(|b| b.is_ascii_digit())
                })
            {
                return Err(AdminError::invalid("API 等价金额无效"));
            }
        }
        let bounded = |value: &str| {
            !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        };
        if !bounded(&self.record_id) || self.revision <= 0 {
            return Err(AdminError::invalid("本地记录 ID 或修订号无效"));
        }
        for value in [
            &self.session_id,
            &self.parent_session_id,
            &self.model,
            &self.reasoning_effort,
            &self.service_tier,
            &self.transport,
        ]
        .into_iter()
        .flatten()
        {
            if !bounded(value) {
                return Err(AdminError::invalid("本地记录字段无效"));
            }
        }
        if self.input_tokens < 0
            || self.output_tokens < 0
            || self.input_tokens.checked_add(self.output_tokens).is_none()
            || self
                .cached_tokens
                .is_some_and(|n| n < 0 || n > self.input_tokens)
            || self
                .reasoning_tokens
                .is_some_and(|n| n < 0 || n > self.output_tokens)
        {
            return Err(AdminError::invalid("本地记录 token 数据无效"));
        }
        if self.duration_ms.is_some_and(|n| n < 0) || self.first_token_ms.is_some_and(|n| n < 0) {
            return Err(AdminError::invalid("本地记录计时无效"));
        }
        Ok(())
    }
}
