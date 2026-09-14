//! 普通用户和同步设备的持久化边界。

use async_trait::async_trait;

use super::store::AdminStoreResult;
use crate::model::{
    observability::TimeRange,
    portal::{PortalCredential, PortalKey, PortalSession, PortalUsageRow, PortalUser},
};

#[async_trait]
pub trait PortalStore: Send + Sync {
    async fn wallet(&self, user_id: &str) -> AdminStoreResult<crate::model::portal::PortalWallet>;
    async fn wallet_events(
        &self,
        user_id: &str,
    ) -> AdminStoreResult<Vec<crate::model::portal::WalletEvent>>;
    async fn set_wallet_policy(
        &self,
        user_id: &str,
        policy: crate::model::portal::WalletPolicy,
    ) -> AdminStoreResult<()>;
    async fn credit_wallet(
        &self,
        user_id: &str,
        operation_id: &str,
        amount: &str,
        note: &str,
    ) -> AdminStoreResult<()>;
    /// 只有验证密码时的会话版本仍有效，才允许更新，避免覆盖管理员同时进行的重置。
    async fn change_password(
        &self,
        id: &str,
        session_version: i64,
        password_hash: &str,
    ) -> AdminStoreResult<bool>;
    async fn own_keys(&self, user_id: &str) -> AdminStoreResult<Vec<PortalKey>>;
    async fn own_usage(
        &self,
        user_id: &str,
        range: TimeRange,
        offset: i64,
    ) -> AdminStoreResult<Vec<PortalUsageRow>>;
    async fn user_credentials(&self, username: &str) -> AdminStoreResult<Option<PortalCredential>>;
    async fn user(&self, id: &str) -> AdminStoreResult<Option<PortalUser>>;
    async fn users(&self) -> AdminStoreResult<Vec<PortalUser>>;
    async fn create_user(&self, credential: PortalCredential) -> AdminStoreResult<()>;
    /// 停用和密码重置必须递增会话版本，防止旧 Cookie 继续使用。
    async fn update_user(
        &self,
        id: &str,
        enabled: bool,
        password_hash: Option<&str>,
    ) -> AdminStoreResult<()>;
    async fn store_session(&self, token_hash: &str, session: PortalSession)
    -> AdminStoreResult<()>;
    async fn session(&self, token_hash: &str) -> AdminStoreResult<Option<PortalSession>>;
    async fn delete_session(&self, token_hash: &str) -> AdminStoreResult<()>;
    async fn assign_key(&self, key_id: &str, user_id: &str) -> AdminStoreResult<()>;
    async fn owned_key_ids(&self, user_id: &str) -> AdminStoreResult<Vec<String>>;
}
