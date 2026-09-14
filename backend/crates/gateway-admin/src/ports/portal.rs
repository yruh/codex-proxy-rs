//! 普通用户和同步设备的持久化边界。

use async_trait::async_trait;

use super::store::AdminStoreResult;
use crate::model::portal::{PortalCredential, PortalSession, PortalUser};

#[async_trait]
pub trait PortalStore: Send + Sync {
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
