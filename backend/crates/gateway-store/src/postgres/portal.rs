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
    async fn own_keys(
        &self,
        user_id: &str,
    ) -> AdminStoreResult<Vec<gateway_admin::model::portal::PortalKey>> {
        let rows=sqlx::query("select k.id,k.name,k.key,k.enabled from client_api_keys k join portal_key_owners o on o.client_api_key_id=k.id join portal_users u on u.id=o.user_id where u.id=$1 and u.enabled order by k.created_at")
            .bind(user_id).fetch_all(&self.pool).await.map_err(failure)?;
        rows.iter()
            .map(|r| {
                Ok(gateway_admin::model::portal::PortalKey {
                    id: r.try_get("id")?,
                    name: r.try_get("name")?,
                    key: r.try_get("key")?,
                    enabled: r.try_get("enabled")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(failure)
    }
    async fn own_usage(
        &self,
        user_id: &str,
        range: gateway_admin::model::observability::TimeRange,
        offset: i64,
    ) -> AdminStoreResult<Vec<gateway_admin::model::portal::PortalUsageRow>> {
        let predicate = super::completed_usage_fact_predicate("r");
        let query = format!(
            "select r.id,o.client_api_key_id,r.upstream_model_id,r.started_at,r.input_tokens,r.output_tokens,r.cached_tokens,r.cost_amount::text as cost from model_requests r join portal_key_owners o on o.client_api_key_id=r.client_api_key_ref join portal_users u on u.id=o.user_id where u.id=$1 and u.enabled and r.started_at >= $2 and r.started_at < $3 and {predicate} order by r.started_at desc,r.id desc limit 100 offset $4"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(user_id)
            .bind(range.start)
            .bind(range.end)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(failure)?;
        rows.iter()
            .map(|r| {
                Ok(gateway_admin::model::portal::PortalUsageRow {
                    id: r.try_get("id")?,
                    key_id: r.try_get("client_api_key_id")?,
                    model: r.try_get("upstream_model_id")?,
                    occurred_at: r.try_get("started_at")?,
                    input_tokens: r.try_get("input_tokens")?,
                    output_tokens: r.try_get("output_tokens")?,
                    cached_tokens: r.try_get("cached_tokens")?,
                    cost: r.try_get("cost")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(failure)
    }
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
