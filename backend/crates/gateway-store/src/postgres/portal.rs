//! 普通用户及其会话的 PostgreSQL 实现。

use async_trait::async_trait;
use gateway_admin::{
    model::portal::{PortalCredential, PortalSession, PortalUser},
    ports::{
        portal::PortalStore,
        store::{AdminStoreError, AdminStoreErrorKind, AdminStoreResult},
    },
};
use sqlx::{PgPool, Row, postgres::PgRow};

pub struct PgPortalStore {
    pool: PgPool,
}

impl PgPortalStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn failure(error: sqlx::Error) -> AdminStoreError {
    let kind = match &error {
        sqlx::Error::Database(e) if e.is_unique_violation() => AdminStoreErrorKind::DuplicateName,
        sqlx::Error::Database(e) if e.is_foreign_key_violation() => AdminStoreErrorKind::NotFound,
        _ => AdminStoreErrorKind::Unavailable,
    };
    AdminStoreError::new(kind, "portal", "普通用户存储操作失败")
}

fn user(row: &PgRow) -> Result<PortalUser, sqlx::Error> {
    Ok(PortalUser {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        enabled: row.try_get("enabled")?,
        session_version: row.try_get("session_version")?,
    })
}

#[async_trait]
impl PortalStore for PgPortalStore {
    async fn user_credentials(&self, username: &str) -> AdminStoreResult<Option<PortalCredential>> {
        let row = sqlx::query("select id, username, enabled, session_version, password_hash from portal_users where username=$1")
            .bind(username).fetch_optional(&self.pool).await.map_err(failure)?;
        row.map(|r| {
            Ok(PortalCredential {
                user: user(&r)?,
                password_hash: r.try_get("password_hash")?,
            })
        })
        .transpose()
        .map_err(failure)
    }
    async fn user(&self, id: &str) -> AdminStoreResult<Option<PortalUser>> {
        sqlx::query("select id, username, enabled, session_version from portal_users where id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(failure)?
            .as_ref()
            .map(user)
            .transpose()
            .map_err(failure)
    }
    async fn users(&self) -> AdminStoreResult<Vec<PortalUser>> {
        sqlx::query("select id, username, enabled, session_version from portal_users order by username limit 1000")
            .fetch_all(&self.pool).await.map_err(failure)?.iter().map(user).collect::<Result<Vec<_>,_>>().map_err(failure)
    }
    async fn create_user(&self, credential: PortalCredential) -> AdminStoreResult<()> {
        sqlx::query(
            "insert into portal_users(id,username,password_hash,enabled) values($1,$2,$3,$4)",
        )
        .bind(credential.user.id)
        .bind(credential.user.username)
        .bind(credential.password_hash)
        .bind(credential.user.enabled)
        .execute(&self.pool)
        .await
        .map_err(failure)?;
        Ok(())
    }
    async fn update_user(
        &self,
        id: &str,
        enabled: bool,
        password_hash: Option<&str>,
    ) -> AdminStoreResult<()> {
        let n=sqlx::query("update portal_users set enabled=$2,password_hash=coalesce($3,password_hash),session_version=session_version+1,updated_at=now() where id=$1")
            .bind(id).bind(enabled).bind(password_hash).execute(&self.pool).await.map_err(failure)?.rows_affected();
        if n == 0 {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::NotFound,
                "portal",
                "用户不存在",
            ));
        }
        Ok(())
    }
    async fn store_session(
        &self,
        token_hash: &str,
        session: PortalSession,
    ) -> AdminStoreResult<()> {
        sqlx::query("insert into portal_sessions(token_hash,user_id,session_version,expires_at) values($1,$2,$3,$4)")
            .bind(token_hash).bind(session.user_id).bind(session.session_version).bind(session.expires_at)
            .execute(&self.pool).await.map_err(failure)?;
        Ok(())
    }
    async fn session(&self, token_hash: &str) -> AdminStoreResult<Option<PortalSession>> {
        let row=sqlx::query("select user_id,session_version,expires_at from portal_sessions where token_hash=$1 and expires_at>now()")
            .bind(token_hash).fetch_optional(&self.pool).await.map_err(failure)?;
        row.map(|r| {
            Ok(PortalSession {
                user_id: r.try_get("user_id")?,
                session_version: r.try_get("session_version")?,
                expires_at: r.try_get("expires_at")?,
            })
        })
        .transpose()
        .map_err(failure)
    }
    async fn delete_session(&self, token_hash: &str) -> AdminStoreResult<()> {
        sqlx::query("delete from portal_sessions where token_hash=$1")
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(failure)?;
        Ok(())
    }
    async fn assign_key(&self, key_id: &str, user_id: &str) -> AdminStoreResult<()> {
        sqlx::query("insert into portal_key_owners(client_api_key_id,user_id) values($1,$2) on conflict(client_api_key_id) do update set user_id=excluded.user_id")
            .bind(key_id).bind(user_id).execute(&self.pool).await.map_err(failure)?;
        Ok(())
    }
    async fn owned_key_ids(&self, user_id: &str) -> AdminStoreResult<Vec<String>> {
        sqlx::query_scalar("select client_api_key_id from portal_key_owners where user_id=$1 order by client_api_key_id")
            .bind(user_id).fetch_all(&self.pool).await.map_err(failure)
    }
}
