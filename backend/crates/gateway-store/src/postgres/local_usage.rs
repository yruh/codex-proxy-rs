//! 独立本地账本；不会更新 model_requests 或密钥扣费表。

use async_trait::async_trait;
use gateway_admin::{
    model::local_usage::LocalUsageRecord,
    ports::{
        local_usage::{LocalUsageStore, UsageSyncDevice},
        store::{AdminStoreError, AdminStoreErrorKind, AdminStoreResult},
    },
};
use sqlx::{PgPool, Row, postgres::PgRow};

pub struct PgLocalUsageStore {
    pool: PgPool,
}
impl PgLocalUsageStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
fn error(_: sqlx::Error) -> AdminStoreError {
    AdminStoreError::new(
        AdminStoreErrorKind::Unavailable,
        "local usage",
        "本地账本操作失败",
    )
}
fn device(row: &PgRow) -> Result<UsageSyncDevice, sqlx::Error> {
    Ok(UsageSyncDevice {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        provider_account_id: row.try_get("provider_account_id")?,
        enabled: row.try_get("enabled")?,
        last_sync_at: row.try_get("last_sync_at")?,
    })
}
#[async_trait]
impl LocalUsageStore for PgLocalUsageStore {
    async fn daily_usage(
        &self,
        range: gateway_admin::model::observability::TimeRange,
        account_id: Option<&str>,
    ) -> AdminStoreResult<Vec<gateway_admin::ports::local_usage::DailySourceUsage>> {
        let predicate = super::completed_usage_fact_predicate("r");
        let query = format!(
            "with facts as (select 'local' as source,r.occurred_at as at,r.input_tokens as input,r.output_tokens as output,r.cached_tokens as cached,r.estimated_usd as cost from local_usage_records r join usage_sync_devices d on d.id=r.device_id where not r.excluded and r.occurred_at >= $1 and r.occurred_at < $2 and ($3::text is null or d.provider_account_id=$3) union all select 'proxy',r.started_at,r.input_tokens,r.output_tokens,r.cached_tokens,r.cost_amount from model_requests r where r.started_at >= $1 and r.started_at < $2 and ($3::text is null or r.provider_account_ref=$3) and {predicate}) select (at at time zone 'Asia/Shanghai')::date::text as day,source,count(*) as requests,coalesce(sum(input),0)::text as input,coalesce(sum(output),0)::text as output,coalesce(sum(cached),0)::text as cached,sum(cost)::text as cost,count(cost) as priced_requests from facts group by 1,2 order by 1,2"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(range.start)
            .bind(range.end)
            .bind(account_id)
            .fetch_all(&self.pool)
            .await
            .map_err(error)?;
        rows.iter()
            .map(|r| {
                Ok(gateway_admin::ports::local_usage::DailySourceUsage {
                    day: r.try_get("day")?,
                    source: r.try_get("source")?,
                    requests: r.try_get("requests")?,
                    input: r.try_get("input")?,
                    output: r.try_get("output")?,
                    cached: r.try_get("cached")?,
                    estimated_usd: r.try_get("cost")?,
                    priced_requests: r.try_get("priced_requests")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(error)
    }
    async fn device_by_token_hash(
        &self,
        token_hash: &str,
    ) -> AdminStoreResult<Option<UsageSyncDevice>> {
        sqlx::query("select id,name,provider_account_id,enabled,last_sync_at from usage_sync_devices where token_hash=$1 and enabled")
            .bind(token_hash).fetch_optional(&self.pool).await.map_err(error)?.as_ref().map(device).transpose().map_err(error)
    }
    async fn devices(&self) -> AdminStoreResult<Vec<UsageSyncDevice>> {
        sqlx::query("select id,name,provider_account_id,enabled,last_sync_at from usage_sync_devices order by created_at limit 1000")
            .fetch_all(&self.pool).await.map_err(error)?.iter().map(device).collect::<Result<Vec<_>,_>>().map_err(error)
    }
    async fn create_device(
        &self,
        device: UsageSyncDevice,
        token_hash: &str,
    ) -> AdminStoreResult<()> {
        sqlx::query("insert into usage_sync_devices(id,name,provider_account_id,token_hash,enabled) values($1,$2,$3,$4,$5)")
            .bind(device.id).bind(device.name).bind(device.provider_account_id).bind(token_hash).bind(device.enabled)
            .execute(&self.pool).await.map_err(error)?;
        Ok(())
    }
    async fn set_device_enabled(&self, id: &str, enabled: bool) -> AdminStoreResult<()> {
        let n = sqlx::query("update usage_sync_devices set enabled=$2 where id=$1")
            .bind(id)
            .bind(enabled)
            .execute(&self.pool)
            .await
            .map_err(error)?
            .rows_affected();
        if n == 0 {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::NotFound,
                "local usage",
                "同步设备不存在",
            ));
        }
        Ok(())
    }
    async fn ingest(&self, device_id: &str, records: &[LocalUsageRecord]) -> AdminStoreResult<u64> {
        if records.len() > 1000 || records.iter().any(|r| r.validate().is_err()) {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::Invalid,
                "local usage",
                "同步批次无效",
            ));
        }
        let mut tx = self.pool.begin().await.map_err(error)?;
        // 与撤销设备串行化，避免鉴权通过后设备已撤销仍写入。
        let enabled: Option<bool> =
            sqlx::query_scalar("select enabled from usage_sync_devices where id=$1 for update")
                .bind(device_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(error)?;
        if enabled != Some(true) {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::NotFound,
                "local usage",
                "同步设备不可用",
            ));
        }
        let mut changed = 0;
        for r in records {
            changed+=sqlx::query("insert into local_usage_records(device_id,record_id,revision,occurred_at,session_id,parent_session_id,model,reasoning_effort,service_tier,transport,input_tokens,output_tokens,cached_tokens,reasoning_tokens,duration_ms,first_token_ms,excluded,estimated_usd) values($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18::text::numeric) on conflict(device_id,record_id) do update set revision=excluded.revision,occurred_at=excluded.occurred_at,session_id=excluded.session_id,parent_session_id=excluded.parent_session_id,model=excluded.model,reasoning_effort=excluded.reasoning_effort,service_tier=excluded.service_tier,transport=excluded.transport,input_tokens=excluded.input_tokens,output_tokens=excluded.output_tokens,cached_tokens=excluded.cached_tokens,reasoning_tokens=excluded.reasoning_tokens,duration_ms=excluded.duration_ms,first_token_ms=excluded.first_token_ms,excluded=excluded.excluded,estimated_usd=excluded.estimated_usd,updated_at=now() where local_usage_records.revision<excluded.revision")
                .bind(device_id).bind(&r.record_id).bind(r.revision).bind(r.occurred_at).bind(&r.session_id).bind(&r.parent_session_id).bind(&r.model)
                .bind(&r.reasoning_effort).bind(&r.service_tier).bind(&r.transport).bind(r.input_tokens).bind(r.output_tokens).bind(r.cached_tokens)
                .bind(r.reasoning_tokens).bind(r.duration_ms).bind(r.first_token_ms).bind(r.excluded).bind(r.estimated_usd.as_deref())
                .execute(&mut *tx).await.map_err(error)?.rows_affected();
        }
        sqlx::query("update usage_sync_devices set last_sync_at=now() where id=$1")
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map_err(error)?;
        tx.commit().await.map_err(error)?;
        Ok(changed)
    }
}
