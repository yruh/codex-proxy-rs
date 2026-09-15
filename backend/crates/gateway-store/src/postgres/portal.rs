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
    async fn delete_user(&self, id: &str) -> AdminStoreResult<gateway_admin::model::Revision> {
        let mut tx = self.pool.begin().await.map_err(failure)?;
        let revision = super::bump_config_revision_in_transaction(&mut tx)
            .await
            .map_err(|_| {
                AdminStoreError::new(AdminStoreErrorKind::Unavailable, "user", "删除用户失败")
            })?;
        let changed = sqlx::query("update portal_users set enabled=false,deleted_at=now(),session_version=session_version+1,updated_at=now() where id=$1 and deleted_at is null")
            .bind(id).execute(&mut *tx).await.map_err(failure)?.rows_affected();
        if changed == 0 {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::NotFound,
                "user",
                "用户不存在或已删除",
            ));
        }
        sqlx::query("delete from client_api_keys where id in (select client_api_key_id from portal_key_owners where user_id=$1)")
            .bind(id).execute(&mut *tx).await.map_err(failure)?;
        sqlx::query("delete from portal_sessions where user_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(failure)?;
        tx.commit().await.map_err(failure)?;
        crate::value::admin_revision(revision)
    }
    async fn delete_own_key(
        &self,
        user: &PortalUser,
        key_id: &str,
    ) -> AdminStoreResult<gateway_admin::model::Revision> {
        let mut tx = self.pool.begin().await.map_err(failure)?;
        let revision = super::bump_config_revision_in_transaction(&mut tx)
            .await
            .map_err(|_| {
                AdminStoreError::new(AdminStoreErrorKind::Unavailable, "key", "删除密钥失败")
            })?;
        let valid: Option<bool> = sqlx::query_scalar("select enabled and deleted_at is null and session_version=$2 from portal_users where id=$1 for update")
            .bind(&user.id).bind(user.session_version).fetch_optional(&mut *tx).await.map_err(failure)?;
        if valid != Some(true) {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::Invalid,
                "user",
                "用户登录已失效",
            ));
        }
        let changed = sqlx::query("delete from client_api_keys where id=$1 and exists(select 1 from portal_key_owners where client_api_key_id=$1 and user_id=$2)")
            .bind(key_id).bind(&user.id).execute(&mut *tx).await.map_err(failure)?.rows_affected();
        if changed == 0 {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::NotFound,
                "key",
                "密钥不存在或已删除",
            ));
        }
        tx.commit().await.map_err(failure)?;
        crate::value::admin_revision(revision)
    }
    async fn pricing(&self) -> AdminStoreResult<gateway_admin::model::portal::PortalPricing> {
        let value: serde_json::Value = sqlx::query_scalar(
            "select policy from portal_pricing_revisions order by id desc limit 1",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(failure)?;
        serde_json::from_value(value).map_err(|_| {
            AdminStoreError::new(AdminStoreErrorKind::Invalid, "pricing", "计费规则无效")
        })
    }
    async fn set_pricing(
        &self,
        policy: gateway_admin::model::portal::PortalPricing,
    ) -> AdminStoreResult<()> {
        sqlx::query("insert into portal_pricing_revisions(policy) values($1)")
            .bind(serde_json::to_value(policy).map_err(|_| {
                AdminStoreError::new(AdminStoreErrorKind::Invalid, "pricing", "计费规则无效")
            })?)
            .execute(&self.pool)
            .await
            .map_err(failure)?;
        Ok(())
    }
    async fn create_own_key(
        &self,
        user: &PortalUser,
        key: &gateway_admin::model::portal::PortalKey,
    ) -> AdminStoreResult<gateway_admin::model::Revision> {
        let mut tx = self.pool.begin().await.map_err(failure)?;
        // 与控制面写入保持锁序，密钥和归属原子发布，不能短暂成为无主的免费密钥。
        let revision = super::bump_config_revision_in_transaction(&mut tx)
            .await
            .map_err(|_| {
                AdminStoreError::new(AdminStoreErrorKind::Unavailable, "key", "密钥创建失败")
            })?;
        let valid: Option<bool> = sqlx::query_scalar(
            "select enabled and session_version=$2 from portal_users where id=$1 for update",
        )
        .bind(&user.id)
        .bind(user.session_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(failure)?;
        if valid != Some(true) {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::Invalid,
                "key",
                "用户登录已失效",
            ));
        }
        let count: i64 =
            sqlx::query_scalar("select count(*) from portal_key_owners o join client_api_keys k on k.id=o.client_api_key_id where o.user_id=$1")
                .bind(&user.id)
                .fetch_one(&mut *tx)
                .await
                .map_err(failure)?;
        if count >= 20 {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::Invalid,
                "key",
                "每个用户最多保留 20 个密钥，请联系管理员清理",
            ));
        }
        let command = super::NewClientApiKey {
            id: key.id.clone(),
            name: key.id.clone(),
            label: Some(key.name.clone()),
            group_ids: vec![],
            key: key.key.clone(),
            max_concurrency: 0,
            requests_per_minute: 0,
            budget: Default::default(),
        };
        super::insert_client_api_key_in_transaction(&mut tx, &command)
            .await
            .map_err(|_| {
                AdminStoreError::new(AdminStoreErrorKind::Invalid, "key", "密钥创建失败")
            })?;
        sqlx::query("insert into portal_key_owners(client_api_key_id,user_id) values($1,$2)")
            .bind(&key.id)
            .bind(&user.id)
            .execute(&mut *tx)
            .await
            .map_err(failure)?;
        tx.commit().await.map_err(failure)?;
        crate::value::admin_revision(revision)
    }
    async fn wallet(
        &self,
        user_id: &str,
    ) -> AdminStoreResult<gateway_admin::model::portal::PortalWallet> {
        use gateway_admin::model::portal::{PortalWallet, WalletPolicy};
        let r = sqlx::query("select u.id,coalesce(w.balance_usd,0)::text as balance,coalesce(w.total_spent_usd,0)::text as spent,coalesce(w.daily_limit_usd,0)::text as daily_limit,coalesce(w.weekly_limit_usd,0)::text as weekly_limit,coalesce(w.max_concurrency,0) as concurrency,(select coalesce(-sum(amount_usd),0)::text from portal_wallet_events where user_id=u.id and kind='usage' and created_at >= date_trunc('day',now() at time zone 'Asia/Shanghai') at time zone 'Asia/Shanghai') as daily_used,(select coalesce(-sum(amount_usd),0)::text from portal_wallet_events where user_id=u.id and kind='usage' and created_at >= date_trunc('week',now() at time zone 'Asia/Shanghai') at time zone 'Asia/Shanghai') as weekly_used,(select count(*) from portal_user_requests where user_id=u.id and not released and expires_at>now()) as active from portal_users u left join portal_wallets w on w.user_id=u.id where u.id=$1")
            .bind(user_id).fetch_one(&self.pool).await.map_err(failure)?;
        Ok(PortalWallet {
            user_id: user_id.to_owned(),
            balance_usd: r.get("balance"),
            total_spent_usd: r.get("spent"),
            daily_used_usd: r.get("daily_used"),
            weekly_used_usd: r.get("weekly_used"),
            active_requests: r.get("active"),
            policy: WalletPolicy {
                daily_limit_usd: r.get("daily_limit"),
                weekly_limit_usd: r.get("weekly_limit"),
                max_concurrency: u32::try_from(r.get::<i32, _>("concurrency")).unwrap_or_default(),
            },
        })
    }
    async fn wallet_events(
        &self,
        user_id: &str,
    ) -> AdminStoreResult<Vec<gateway_admin::model::portal::WalletEvent>> {
        let rows = sqlx::query("select id,kind,amount_usd::text as amount,note,created_at from portal_wallet_events where user_id=$1 order by created_at desc,id desc limit 100")
            .bind(user_id).fetch_all(&self.pool).await.map_err(failure)?;
        Ok(rows
            .iter()
            .map(|r| gateway_admin::model::portal::WalletEvent {
                id: r.get("id"),
                kind: r.get("kind"),
                amount_usd: r.get("amount"),
                note: r.get("note"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
    async fn set_wallet_policy(
        &self,
        user_id: &str,
        policy: gateway_admin::model::portal::WalletPolicy,
    ) -> AdminStoreResult<()> {
        sqlx::query("insert into portal_wallets(user_id,daily_limit_usd,weekly_limit_usd,max_concurrency) values($1,$2::text::numeric,$3::text::numeric,$4) on conflict(user_id) do update set daily_limit_usd=excluded.daily_limit_usd,weekly_limit_usd=excluded.weekly_limit_usd,max_concurrency=excluded.max_concurrency,updated_at=now()")
            .bind(user_id).bind(policy.daily_limit_usd).bind(policy.weekly_limit_usd).bind(i32::try_from(policy.max_concurrency).unwrap_or(i32::MAX)).execute(&self.pool).await.map_err(failure)?;
        Ok(())
    }
    async fn credit_wallet(
        &self,
        user_id: &str,
        operation_id: &str,
        amount: &str,
        note: &str,
    ) -> AdminStoreResult<()> {
        let mut tx = self.pool.begin().await.map_err(failure)?;
        sqlx::query("insert into portal_wallets(user_id) values($1) on conflict do nothing")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(failure)?;
        sqlx::query("select user_id from portal_wallets where user_id=$1 for update")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(failure)?;
        let id = format!("credit:{operation_id}");
        let changed = sqlx::query("insert into portal_wallet_events(id,user_id,kind,amount_usd,note) values($1,$2,'credit',$3::text::numeric,$4) on conflict(id) do nothing")
            .bind(&id).bind(user_id).bind(amount).bind(note).execute(&mut *tx).await.map_err(failure)?.rows_affected();
        if changed == 1 {
            sqlx::query("update portal_wallets set balance_usd=balance_usd+$2::text::numeric,updated_at=now() where user_id=$1").bind(user_id).bind(amount).execute(&mut *tx).await.map_err(failure)?;
        } else {
            let matches:bool=sqlx::query_scalar("select user_id=$2 and amount_usd=$3::text::numeric and note=$4 from portal_wallet_events where id=$1").bind(&id).bind(user_id).bind(amount).bind(note).fetch_one(&mut *tx).await.map_err(failure)?;
            if !matches {
                return Err(AdminStoreError::new(
                    AdminStoreErrorKind::DuplicateName,
                    "portal",
                    "充值编号已用于另一笔操作",
                ));
            }
        }
        tx.commit().await.map_err(failure)
    }
    async fn change_password(
        &self,
        id: &str,
        session_version: i64,
        password_hash: &str,
    ) -> AdminStoreResult<bool> {
        let changed = sqlx::query("update portal_users set password_hash=$3,session_version=session_version+1,updated_at=now() where id=$1 and session_version=$2 and enabled")
            .bind(id).bind(session_version).bind(password_hash).execute(&self.pool).await.map_err(failure)?.rows_affected();
        Ok(changed == 1)
    }
    async fn own_keys(
        &self,
        user_id: &str,
    ) -> AdminStoreResult<Vec<gateway_admin::model::portal::PortalKey>> {
        let rows=sqlx::query("select k.id,coalesce(k.label,k.name) as name,k.key,k.enabled from client_api_keys k join portal_key_owners o on o.client_api_key_id=k.id join portal_users u on u.id=o.user_id where u.id=$1 and u.enabled order by k.created_at")
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
            "select r.id,o.client_api_key_id,r.upstream_model_id,r.started_at,r.input_tokens,r.output_tokens,r.cached_tokens,(-e.amount_usd)::text as cost from model_requests r left join portal_wallet_events e on e.id='usage:'||r.id and e.kind='usage' join portal_key_owners o on o.client_api_key_id=r.client_api_key_ref join portal_users u on u.id=o.user_id where u.id=$1 and u.enabled and r.started_at >= $2 and r.started_at < $3 and {predicate} order by r.started_at desc,r.id desc limit 100 offset $4"
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
        let row = sqlx::query("select id, username, enabled, session_version, password_hash from portal_users where username=$1 and deleted_at is null")
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
        sqlx::query("select id, username, enabled, session_version from portal_users where id=$1 and deleted_at is null")
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
        sqlx::query("select id, username, enabled, session_version from portal_users where deleted_at is null order by username limit 1000")
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
        let n=sqlx::query("update portal_users set enabled=$2,password_hash=coalesce($3,password_hash),session_version=session_version+1,updated_at=now() where id=$1 and deleted_at is null")
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
        let mut tx = self.pool.begin().await.map_err(failure)?;
        // 与删除、创建密钥保持相同锁序，避免删除后重新产生归属。
        super::bump_config_revision_in_transaction(&mut tx)
            .await
            .map_err(|_| {
                AdminStoreError::new(AdminStoreErrorKind::Unavailable, "key", "绑定密钥失败")
            })?;
        let valid: Option<bool> = sqlx::query_scalar(
            "select deleted_at is null from portal_users where id=$1 for update",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(failure)?;
        let key: Option<String> =
            sqlx::query_scalar("select id from client_api_keys where id=$1 for update")
                .bind(key_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(failure)?;
        if valid != Some(true) || key.is_none() {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::NotFound,
                "key",
                "用户或密钥不存在",
            ));
        }
        let result = sqlx::query("insert into portal_key_owners(client_api_key_id,user_id) values($1,$2) on conflict(client_api_key_id) do update set user_id=excluded.user_id where portal_key_owners.user_id=excluded.user_id")
            .bind(key_id).bind(user_id).execute(&mut *tx).await.map_err(failure)?;
        // 密钥携带历史记录，不能把原用户的历史用量静默转给另一个用户。
        if result.rows_affected() == 0 {
            return Err(AdminStoreError::new(
                AdminStoreErrorKind::DuplicateName,
                "portal",
                "该密钥已属于其他用户，请为新用户创建独立密钥",
            ));
        }
        tx.commit().await.map_err(failure)?;
        Ok(())
    }
    async fn owned_key_ids(&self, user_id: &str) -> AdminStoreResult<Vec<String>> {
        sqlx::query_scalar("select client_api_key_id from portal_key_owners where user_id=$1 order by client_api_key_id")
            .bind(user_id).fetch_all(&self.pool).await.map_err(failure)
    }
}
