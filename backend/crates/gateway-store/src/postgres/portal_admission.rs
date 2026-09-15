//! 用户共享并发和钱包准入；在同一用户行锁内检查全部所属密钥。
use futures::future::BoxFuture;
use gateway_core::{
    engine::{
        ModelRequestId,
        admission::{
            ClientAdmissionDecision, ClientAdmissionError, ClientAdmissionPort,
            ClientAdmissionRecovery, ClientAdmissionRejection, ClientAdmissionRequest,
            ClientAdmissionRestoreResult,
        },
    },
    policy::ClientApiKeyId,
};
use sqlx::{PgPool, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct PgPortalAdmission {
    pool: PgPool,
    inner: Arc<dyn ClientAdmissionPort>,
}
impl PgPortalAdmission {
    pub fn new(pool: PgPool, inner: Arc<dyn ClientAdmissionPort>) -> Self {
        Self { pool, inner }
    }
    async fn admit_user(
        &self,
        request: &ClientAdmissionRequest,
    ) -> Result<ClientAdmissionDecision, ClientAdmissionError> {
        let mut tx = self.pool.begin().await.map_err(|_| ClientAdmissionError)?;
        let owner: Option<String> =
            sqlx::query_scalar("select user_id from portal_key_owners where client_api_key_id=$1")
                .bind(request.client_api_key_id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| ClientAdmissionError)?;
        let Some(owner) = owner else {
            return Ok(ClientAdmissionDecision::Granted);
        };
        // 用户归属不可换绑；此处记录准入时归属，结束后结算不重新查询密钥所有者。
        sqlx::query("insert into portal_wallets(user_id) values($1) on conflict do nothing")
            .bind(&owner)
            .execute(&mut *tx)
            .await
            .map_err(|_| ClientAdmissionError)?;
        let row=sqlx::query("select w.max_concurrency,u.enabled,(w.balance_usd<=0) as empty from portal_wallets w join portal_users u on u.id=w.user_id where w.user_id=$1 for update of w")
            .bind(&owner).fetch_one(&mut *tx).await.map_err(|_|ClientAdmissionError)?;
        if !row.get::<bool, _>("enabled") || row.get::<bool, _>("empty") {
            return Ok(ClientAdmissionDecision::Rejected(
                ClientAdmissionRejection::RateLimited,
            ));
        }
        let exceeded:bool=sqlx::query_scalar("select (daily_limit_usd>0 and daily_limit_usd<=coalesce((select -sum(amount_usd) from portal_wallet_events where user_id=$1 and kind='usage' and created_at>=date_trunc('day',now() at time zone 'Asia/Shanghai') at time zone 'Asia/Shanghai'),0)) or (weekly_limit_usd>0 and weekly_limit_usd<=coalesce((select -sum(amount_usd) from portal_wallet_events where user_id=$1 and kind='usage' and created_at>=date_trunc('week',now() at time zone 'Asia/Shanghai') at time zone 'Asia/Shanghai'),0)) from portal_wallets where user_id=$1")
            .bind(&owner).fetch_one(&mut *tx).await.map_err(|_|ClientAdmissionError)?;
        if exceeded {
            return Ok(ClientAdmissionDecision::Rejected(
                ClientAdmissionRejection::RateLimited,
            ));
        }
        let active:i64=sqlx::query_scalar("select count(*) from portal_user_requests where user_id=$1 and not released and expires_at>now() and request_id<>$2")
            .bind(&owner).bind(request.model_request_id.as_str()).fetch_one(&mut *tx).await.map_err(|_|ClientAdmissionError)?;
        let limit = row.get::<i32, _>("max_concurrency");
        if limit > 0 && active >= i64::from(limit) {
            return Ok(ClientAdmissionDecision::Rejected(
                ClientAdmissionRejection::ConcurrencyLimited,
            ));
        }
        let ttl = i64::try_from(request.lease_ttl.as_millis()).map_err(|_| ClientAdmissionError)?;
        // 密钥排队可能先释放用户槽位再重试；重新准入必须恢复槽位，但保留最初固定的计费版本。
        sqlx::query("insert into portal_user_requests(request_id,user_id,key_id,expires_at,pricing_revision) values($1,$2,$3,now()+$4*interval '1 millisecond',(select max(id) from portal_pricing_revisions)) on conflict(request_id) do update set released=false,expires_at=excluded.expires_at")
            .bind(request.model_request_id.as_str()).bind(&owner).bind(request.client_api_key_id.as_str()).bind(ttl).execute(&mut *tx).await.map_err(|_|ClientAdmissionError)?;
        tx.commit().await.map_err(|_| ClientAdmissionError)?;
        Ok(ClientAdmissionDecision::Granted)
    }
    async fn release_user(&self, request_id: &ModelRequestId) -> Result<(), ClientAdmissionError> {
        sqlx::query("update portal_user_requests set released=true where request_id=$1")
            .bind(request_id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|_| ClientAdmissionError)?;
        Ok(())
    }
}
impl ClientAdmissionPort for PgPortalAdmission {
    fn abandon(&self, key: &ClientApiKeyId, request: &ModelRequestId) {
        let repository = self.clone();
        let key = key.clone();
        let request = request.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            drop(runtime.spawn(async move {
                if let Err(error) = repository.release(&key, &request).await {
                    tracing::warn!(%error, "已取消用户准入的租约释放失败，依赖 TTL 收敛");
                }
            }));
        }
    }

    fn admit(
        &self,
        request: ClientAdmissionRequest,
    ) -> BoxFuture<'_, Result<ClientAdmissionDecision, ClientAdmissionError>> {
        Box::pin(async move {
            let decision = self.admit_user(&request).await?;
            if decision != ClientAdmissionDecision::Granted {
                return Ok(decision);
            }
            let id = request.model_request_id.clone();
            let result = self.inner.admit(request).await;
            if !matches!(result, Ok(ClientAdmissionDecision::Granted)) {
                self.release_user(&id).await?;
            }
            result
        })
    }
    fn release<'a>(
        &'a self,
        key: &'a ClientApiKeyId,
        id: &'a ModelRequestId,
    ) -> BoxFuture<'a, Result<bool, ClientAdmissionError>> {
        Box::pin(async move {
            self.release_user(id).await?;
            self.inner.release(key, id).await
        })
    }
    fn restore(
        &self,
        recovery: ClientAdmissionRecovery,
    ) -> BoxFuture<'_, Result<ClientAdmissionRestoreResult, ClientAdmissionError>> {
        // 用户槽位直接持久化在 PG，不依赖 Redis 恢复；TTL 与请求截止时间一致。
        self.inner.restore(recovery)
    }
}
