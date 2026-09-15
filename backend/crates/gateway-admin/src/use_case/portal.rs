//! 普通用户认证与授权。管理员必须由 HTTP 管理鉴权入口另行验证。

use crate::{
    model::{
        AdminError, AdminErrorKind,
        portal::{PortalCredential, PortalSession, PortalUser, normalize_username},
    },
    ports::portal::PortalStore,
};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, Instant},
};
use uuid::Uuid;

/// 仅供普通用户门户使用，不签发管理员会话。
pub struct PortalService {
    store: Arc<dyn PortalStore>,
    snapshot: Arc<dyn gateway_core::runtime::SnapshotControl>,
    attempts: Mutex<HashMap<String, (Instant, u32)>>,
    password_slots: Arc<tokio::sync::Semaphore>,
}

impl PortalService {
    pub async fn delete_user(&self, id: &str) -> Result<(), AdminError> {
        let revision = self.store.delete_user(id).await.map_err(store_error)?;
        super::publish_committed(self.snapshot.as_ref(), revision).await
    }
    pub async fn delete_key(&self, token: &str, key_id: &str) -> Result<(), AdminError> {
        let user = self.current_user(token).await?;
        let revision = self
            .store
            .delete_own_key(&user, key_id)
            .await
            .map_err(store_error)?;
        super::publish_committed(self.snapshot.as_ref(), revision).await
    }
    pub async fn pricing(&self) -> Result<crate::model::portal::PortalPricing, AdminError> {
        self.store.pricing().await.map_err(store_error)
    }
    pub async fn set_pricing(
        &self,
        mut policy: crate::model::portal::PortalPricing,
    ) -> Result<(), AdminError> {
        fn multiplier(value: &str) -> Result<String, AdminError> {
            let n = wallet_amount(value)?;
            let parsed: f64 = n.parse().map_err(|_| AdminError::invalid("倍率无效"))?;
            if !(0.000001..=1000.0).contains(&parsed)
                || n.split('.').nth(1).is_some_and(|v| v.len() > 6)
            {
                return Err(AdminError::invalid(
                    "倍率须为 0.000001 至 1000，最多六位小数",
                ));
            }
            Ok(n)
        }
        policy.global_multiplier = multiplier(&policy.global_multiplier)?;
        if policy.model_multipliers.len() > 200 {
            return Err(AdminError::invalid("最多设置 200 个模型倍率"));
        }
        for (model, value) in &mut policy.model_multipliers {
            if model.trim() != model
                || model.is_empty()
                || model.len() > 200
                || model.chars().any(char::is_control)
            {
                return Err(AdminError::invalid("模型 ID 无效"));
            }
            *value = multiplier(value)?;
        }
        self.store.set_pricing(policy).await.map_err(store_error)
    }
    pub async fn create_key(
        &self,
        token: &str,
        name: &str,
    ) -> Result<crate::model::portal::PortalKey, AdminError> {
        let user = self.current_user(token).await?;
        let name = name.trim();
        if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
            return Err(AdminError::invalid("密钥名称须为 1 至 100 字节"));
        }
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let key = crate::model::portal::PortalKey {
            id: format!("key_{}", Uuid::now_v7().simple()),
            name: name.to_owned(),
            key: format!("sk_{}", URL_SAFE_NO_PAD.encode(bytes)),
            enabled: true,
        };
        let revision = self
            .store
            .create_own_key(&user, &key)
            .await
            .map_err(store_error)?;
        super::publish_committed(self.snapshot.as_ref(), revision).await?;
        Ok(key)
    }
    pub async fn wallet(&self, id: &str) -> Result<crate::model::portal::PortalWallet, AdminError> {
        self.store.wallet(id).await.map_err(store_error)
    }
    pub async fn wallet_events(
        &self,
        id: &str,
    ) -> Result<Vec<crate::model::portal::WalletEvent>, AdminError> {
        self.store.wallet_events(id).await.map_err(store_error)
    }
    pub async fn set_wallet_policy(
        &self,
        id: &str,
        mut policy: crate::model::portal::WalletPolicy,
    ) -> Result<(), AdminError> {
        policy.daily_limit_usd = wallet_amount(&policy.daily_limit_usd)?;
        policy.weekly_limit_usd = wallet_amount(&policy.weekly_limit_usd)?;
        if policy.max_concurrency > 10000 {
            return Err(AdminError::invalid("并发上限不得超过 10000"));
        }
        self.store
            .set_wallet_policy(id, policy)
            .await
            .map_err(store_error)
    }
    pub async fn credit_wallet(
        &self,
        id: &str,
        operation_id: &str,
        amount: &str,
        note: &str,
    ) -> Result<(), AdminError> {
        let amount = wallet_amount(amount)?;
        if amount == "0" || Uuid::parse_str(operation_id).is_err() || note.len() > 500 {
            return Err(AdminError::invalid(
                "充值金额必须大于 0，操作编号必须有效，备注不超过 500 字节",
            ));
        }
        self.store
            .credit_wallet(id, operation_id, &amount, note)
            .await
            .map_err(store_error)
    }
    pub async fn change_password(
        &self,
        token: &str,
        old_password: String,
        new_password: String,
    ) -> Result<(), AdminError> {
        let user = self.current_user(token).await?;
        // 复用登录的密码校验与限流；临时会话立即撤销，修改后所有旧会话一并失效。
        let (verification_token, verified) = self.login(&user.username, old_password).await?;
        self.logout(&verification_token).await?;
        if verified.session_version != user.session_version {
            return Err(unauthorized());
        }
        let hash = self.hash_password(new_password).await?;
        if !self
            .store
            .change_password(&user.id, user.session_version, &hash)
            .await
            .map_err(store_error)?
        {
            return Err(unauthorized());
        }
        Ok(())
    }
    pub async fn keys(
        &self,
        token: &str,
    ) -> Result<Vec<crate::model::portal::PortalKey>, AdminError> {
        let user = self.current_user(token).await?;
        self.store.own_keys(&user.id).await.map_err(store_error)
    }

    pub async fn usage(
        &self,
        token: &str,
        range: crate::model::observability::TimeRange,
        page: u32,
    ) -> Result<Vec<crate::model::portal::PortalUsageRow>, AdminError> {
        if page == 0 || page > 10000 {
            return Err(AdminError::invalid("页码无效"));
        }
        let range = crate::model::observability::TimeRange::new(range.start, range.end)
            .map_err(|_| AdminError::invalid("时间范围无效"))?;
        let user = self.current_user(token).await?;
        self.store
            .own_usage(&user.id, range, i64::from(page - 1) * 100)
            .await
            .map_err(store_error)
    }
    #[must_use]
    pub fn new(
        store: Arc<dyn PortalStore>,
        snapshot: Arc<dyn gateway_core::runtime::SnapshotControl>,
    ) -> Self {
        Self {
            store,
            snapshot,
            attempts: Mutex::new(HashMap::new()),
            password_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }

    pub async fn users(&self) -> Result<Vec<PortalUser>, AdminError> {
        self.store.users().await.map_err(store_error)
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: String,
    ) -> Result<PortalUser, AdminError> {
        let username = normalize_username(username)?;
        let password_hash = self.hash_password(password).await?;
        let user = PortalUser {
            id: format!("user_{}", Uuid::now_v7().simple()),
            username,
            enabled: true,
            session_version: 1,
        };
        self.store
            .create_user(PortalCredential {
                user: user.clone(),
                password_hash,
            })
            .await
            .map_err(store_error)?;
        Ok(user)
    }

    pub async fn update_user(
        &self,
        id: &str,
        enabled: bool,
        password: Option<String>,
    ) -> Result<(), AdminError> {
        let hash = match password {
            Some(p) => Some(self.hash_password(p).await?),
            None => None,
        };
        self.store
            .update_user(id, enabled, hash.as_deref())
            .await
            .map_err(store_error)
    }

    async fn hash_password(&self, password: String) -> Result<String, AdminError> {
        if password.len() < 12 || password.len() > 1024 {
            return Err(AdminError::invalid("密码长度必须为 12 至 1024 字节"));
        }
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| busy())?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            Argon2::default()
                .hash_password(password.as_bytes())
                .map(|h| h.to_string())
                .map_err(|_| unavailable())
        })
        .await
        .map_err(|_| unavailable())?
    }

    pub async fn login(
        &self,
        username: &str,
        password: String,
    ) -> Result<(String, PortalUser), AdminError> {
        let username = normalize_username(username).map_err(|_| unauthorized())?;
        if password.len() > 1024 {
            return Err(unauthorized());
        }
        // 限制每个用户名尝试次数，并限制密码哈希并发，避免占满异步执行器。
        {
            let mut attempts = self.attempts.lock().map_err(|_| unavailable())?;
            attempts.retain(|_, (at, _)| at.elapsed() < StdDuration::from_secs(900));
            if attempts.len() >= 4096 && !attempts.contains_key(&username) {
                return Err(busy());
            }
            let entry = attempts
                .entry(username.clone())
                .or_insert((Instant::now(), 0));
            if entry.1 >= 10 {
                return Err(busy());
            }
            entry.1 += 1;
        }
        let credential = self
            .store
            .user_credentials(&username)
            .await
            .map_err(store_error)?
            .ok_or_else(unauthorized)?;
        if !credential.user.enabled {
            return Err(unauthorized());
        }
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| busy())?;
        let valid = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            PasswordHash::new(&credential.password_hash)
                .ok()
                .is_some_and(|hash| {
                    Argon2::default()
                        .verify_password(password.as_bytes(), &hash)
                        .is_ok()
                })
        })
        .await
        .map_err(|_| unavailable())?;
        if !valid {
            return Err(unauthorized());
        }
        let user = self
            .store
            .user(&credential.user.id)
            .await
            .map_err(store_error)?
            .ok_or_else(unauthorized)?;
        if !user.enabled || user.session_version != credential.user.session_version {
            return Err(unauthorized());
        }
        let mut random = [0u8; 32];
        OsRng.fill_bytes(&mut random);
        let token = URL_SAFE_NO_PAD.encode(random);
        self.store
            .store_session(
                &token_hash(&token),
                PortalSession {
                    user_id: user.id.clone(),
                    session_version: user.session_version,
                    expires_at: Utc::now() + Duration::hours(24),
                },
            )
            .await
            .map_err(store_error)?;
        self.attempts
            .lock()
            .map_err(|_| unavailable())?
            .remove(&username);
        Ok((token, user))
    }

    pub async fn current_user(&self, token: &str) -> Result<PortalUser, AdminError> {
        if token.len() != 43 {
            return Err(unauthorized());
        }
        let session = self
            .store
            .session(&token_hash(token))
            .await
            .map_err(store_error)?
            .ok_or_else(unauthorized)?;
        let user = self
            .store
            .user(&session.user_id)
            .await
            .map_err(store_error)?
            .ok_or_else(unauthorized)?;
        if !session.authorizes(&user, Utc::now()) {
            return Err(unauthorized());
        }
        Ok(user)
    }

    pub async fn logout(&self, token: &str) -> Result<(), AdminError> {
        self.store
            .delete_session(&token_hash(token))
            .await
            .map_err(store_error)
    }

    pub async fn assign_key(&self, key_id: &str, user_id: &str) -> Result<(), AdminError> {
        self.store
            .assign_key(key_id, user_id)
            .await
            .map_err(store_error)
    }

    pub async fn owned_keys(&self, token: &str) -> Result<Vec<String>, AdminError> {
        let user = self.current_user(token).await?;
        self.store
            .owned_key_ids(&user.id)
            .await
            .map_err(store_error)
    }

    pub async fn require_key(&self, token: &str, key_id: &str) -> Result<(), AdminError> {
        if !self.owned_keys(token).await?.iter().any(|id| id == key_id) {
            return Err(unauthorized());
        }
        Ok(())
    }
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn wallet_amount(value: &str) -> Result<String, AdminError> {
    if value.is_empty()
        || value.len() > 24
        || value.starts_with('-')
        || !value.chars().all(|c| c.is_ascii_digit() || c == '.')
        || value.split('.').nth(1).is_some_and(|s| s.len() > 8)
    {
        return Err(AdminError::invalid("USD 金额必须为非负数，最多 8 位小数"));
    }
    let amount: gateway_core::metering::Decimal = value
        .parse()
        .map_err(|_| AdminError::invalid("USD 金额无效"))?;
    Ok(amount.canonical())
}
fn unauthorized() -> AdminError {
    AdminError::new(AdminErrorKind::Unauthorized, "登录信息无效或无权访问")
}
fn unavailable() -> AdminError {
    AdminError::new(AdminErrorKind::Unavailable, "用户服务暂不可用")
}
fn busy() -> AdminError {
    AdminError::new(AdminErrorKind::RateLimited, "登录尝试过多，请稍后重试")
}
fn store_error(error: crate::ports::store::AdminStoreError) -> AdminError {
    super::map_store_error(error, "portal")
}
