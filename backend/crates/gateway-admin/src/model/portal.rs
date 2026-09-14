//! 普通用户身份及密钥归属，不能转换为管理员身份。

use chrono::{DateTime, Utc};

use super::AdminError;

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn session_requires_owner_enabled_version_and_expiry() {
        let now = Utc::now();
        let user = PortalUser {
            id: "a".into(),
            username: "alice".into(),
            enabled: true,
            session_version: 1,
        };
        let session = PortalSession {
            user_id: "a".into(),
            session_version: 1,
            expires_at: now + Duration::hours(1),
        };
        assert!(session.authorizes(&user, now));
        assert!(!session.authorizes(
            &PortalUser {
                id: "b".into(),
                ..user.clone()
            },
            now
        ));
        assert!(!session.authorizes(
            &PortalUser {
                enabled: false,
                ..user.clone()
            },
            now
        ));
        assert!(!session.authorizes(
            &PortalUser {
                session_version: 2,
                ..user
            },
            now
        ));
        assert!(!session.authorizes(
            &PortalUser {
                id: "a".into(),
                username: "alice".into(),
                enabled: true,
                session_version: 1
            },
            session.expires_at
        ));
    }
}
