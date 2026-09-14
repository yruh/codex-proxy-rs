//! 同步凭据只绑定一个设备，不接受上传方指定其他设备身份。

use crate::{
    model::{AdminError, AdminErrorKind, local_usage::LocalUsageRecord},
    ports::local_usage::{LocalUsageStore, UsageSyncDevice},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

pub struct LocalUsageService {
    store: Arc<dyn LocalUsageStore>,
}
impl LocalUsageService {
    pub async fn daily_usage(
        &self,
        range: crate::model::observability::TimeRange,
        account_id: Option<&str>,
    ) -> Result<Vec<crate::ports::local_usage::DailySourceUsage>, AdminError> {
        let range = crate::model::observability::TimeRange::new(range.start, range.end)
            .map_err(|_| AdminError::invalid("时间范围无效"))?;
        self.store
            .daily_usage(range, account_id)
            .await
            .map_err(|e| super::map_store_error(e, "combined usage"))
    }
    #[must_use]
    pub fn new(store: Arc<dyn LocalUsageStore>) -> Self {
        Self { store }
    }
    pub async fn devices(&self) -> Result<Vec<UsageSyncDevice>, AdminError> {
        self.store
            .devices()
            .await
            .map_err(|e| super::map_store_error(e, "sync devices"))
    }
    pub async fn create_device(
        &self,
        name: String,
        account_id: String,
    ) -> Result<(UsageSyncDevice, String), AdminError> {
        if name.trim().is_empty()
            || name.len() > 100
            || account_id.is_empty()
            || account_id.len() > 256
        {
            return Err(AdminError::invalid("设备名称或账号无效"));
        }
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let device = UsageSyncDevice {
            id: format!("device_{}", Uuid::now_v7().simple()),
            name,
            provider_account_id: account_id,
            enabled: true,
            last_sync_at: None,
        };
        self.store
            .create_device(device.clone(), &digest(&token))
            .await
            .map_err(|e| super::map_store_error(e, "sync device"))?;
        Ok((device, token))
    }
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), AdminError> {
        self.store
            .set_device_enabled(id, enabled)
            .await
            .map_err(|e| super::map_store_error(e, "sync device"))
    }
    pub async fn ingest(
        &self,
        token: &str,
        records: &[LocalUsageRecord],
    ) -> Result<u64, AdminError> {
        if records.len() > 1000 {
            return Err(AdminError::invalid("每批最多同步 1000 条记录"));
        }
        for row in records {
            row.validate()?;
        }
        let device = self.authenticate(token).await?;
        self.store
            .ingest(&device.id, records)
            .await
            .map_err(|e| super::map_store_error(e, "local usage"))
    }
    pub async fn authenticate(&self, token: &str) -> Result<UsageSyncDevice, AdminError> {
        if token.len() != 43 {
            return Err(unauthorized());
        }
        self.store
            .device_by_token_hash(&digest(token))
            .await
            .map_err(|e| super::map_store_error(e, "sync device"))?
            .filter(|d| d.enabled)
            .ok_or_else(unauthorized)
    }
}
fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn unauthorized() -> AdminError {
    AdminError::new(AdminErrorKind::Unauthorized, "同步凭据无效或已停用")
}
