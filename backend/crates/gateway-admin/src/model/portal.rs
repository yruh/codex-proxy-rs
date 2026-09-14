//! 普通用户身份及密钥归属，不能转换为管理员身份。

use chrono::{DateTime, Utc};

use super::AdminError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WalletPolicy {
    pub daily_limit_usd: String,
    pub weekly_limit_usd: String,
    pub max_concurrency: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalWallet {
    pub user_id: String,
    pub balance_usd: String,
    pub total_spent_usd: String,
    pub daily_used_usd: String,
    pub weekly_used_usd: String,
    pub weekly_resets_at: DateTime<Utc>,
    pub active_requests: i64,
    #[serde(flatten)]
    pub policy: WalletPolicy,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletEvent {
    pub id: String,
    pub kind: String,
    pub amount_usd: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

/// 只向所属用户返回密钥；不派生 Debug，避免令牌进入日志。
pub struct PortalKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct PortalUsageRow {
    pub id: String,
    pub key_id: String,
    pub model: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cost: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalUser {
    pub id: String,
    pub username: String,
    pub enabled: bool,
    pub session_version: i64,
}

#[derive(Clone)]
pub struct PortalCredential {
    pub user: PortalUser,
    pub password_hash: String,
}

impl std::fmt::Debug for PortalCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortalCredential")
            .field("user", &self.user)
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalSession {
    pub user_id: String,
    pub session_version: i64,
    pub expires_at: DateTime<Utc>,
}

impl PortalSession {
    #[must_use]
    pub fn authorizes(&self, user: &PortalUser, now: DateTime<Utc>) -> bool {
        user.enabled
            && self.user_id == user.id
            && self.session_version == user.session_version
            && self.expires_at > now
    }
}

/// 用户名只做大小写归一化，不接受不可见控制字符或超长值。
///
/// # Errors
///
/// 空白、控制字符或长度超限时拒绝。
pub fn normalize_username(value: &str) -> Result<String, AdminError> {
    let value = value.trim().to_lowercase();
    if value.is_empty() || value.len() > 100 || value.chars().any(char::is_control) {
        return Err(AdminError::invalid(
            "用户名必须为 1 至 100 字节且不包含控制字符",
        ));
    }
    Ok(value)
}
