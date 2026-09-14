//! 本地账本同步的持久化边界。

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::store::AdminStoreResult;
use crate::model::local_usage::LocalUsageRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSyncDevice {
    pub id: String,
    pub name: String,
    pub provider_account_id: String,
    pub enabled: bool,
    pub last_sync_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait LocalUsageStore: Send + Sync {
    async fn daily_usage(
        &self,
        range: crate::model::observability::TimeRange,
        account_id: Option<&str>,
    ) -> AdminStoreResult<Vec<DailySourceUsage>>;
    async fn device_by_token_hash(
        &self,
        token_hash: &str,
    ) -> AdminStoreResult<Option<UsageSyncDevice>>;
    async fn devices(&self) -> AdminStoreResult<Vec<UsageSyncDevice>>;
    async fn create_device(
        &self,
        device: UsageSyncDevice,
        token_hash: &str,
    ) -> AdminStoreResult<()>;
    async fn set_device_enabled(&self, id: &str, enabled: bool) -> AdminStoreResult<()>;
    /// 同一批次原子提交，仅更新版本更高的记录。
    async fn ingest(&self, device_id: &str, records: &[LocalUsageRecord]) -> AdminStoreResult<u64>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailySourceUsage {
    pub day: String,
    pub source: String,
    pub requests: i64,
    pub input: String,
    pub output: String,
    pub cached: String,
    pub estimated_usd: Option<String>,
    pub priced_requests: i64,
}
