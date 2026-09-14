use super::TestDatabase;
use chrono::{Duration, Utc};
use futures::future::BoxFuture;
use gateway_admin::{
    model::portal::{PortalCredential, PortalSession, PortalUser},
    ports::portal::PortalStore,
};
use gateway_core::engine::admission::*;
use gateway_core::{engine::ModelRequestId, policy::ClientApiKeyId};
use gateway_store::postgres::PgPortalStore;

struct AllowAdmission;
impl ClientAdmissionPort for AllowAdmission {
    fn admit(
        &self,
        _: ClientAdmissionRequest,
    ) -> BoxFuture<'_, Result<ClientAdmissionDecision, ClientAdmissionError>> {
        Box::pin(async { Ok(ClientAdmissionDecision::Granted) })
    }
    fn release<'a>(
        &'a self,
        _: &'a ClientApiKeyId,
        _: &'a ModelRequestId,
    ) -> BoxFuture<'a, Result<bool, ClientAdmissionError>> {
        Box::pin(async { Ok(true) })
    }
    fn restore(
        &self,
        _: ClientAdmissionRecovery,
    ) -> BoxFuture<'_, Result<ClientAdmissionRestoreResult, ClientAdmissionError>> {
        Box::pin(async { Ok(ClientAdmissionRestoreResult::default()) })
    }
}

#[tokio::test]
async fn wallet_shares_concurrency_and_charges_once_across_keys() {
    use gateway_admin::model::portal::WalletPolicy;
    use gateway_core::engine::budget::{ClientBudgetCharge, ClientBudgetPort};
    use gateway_store::postgres::{PgClientBudgetStore, PgPortalAdmission};
    use std::{
        sync::Arc,
        time::{Duration as StdDuration, SystemTime},
    };
    let Some(database) = TestDatabase::create("portal_wallet").await else {
        return;
    };
    let store = PgPortalStore::new(database.pool.clone());
    store
        .create_user(PortalCredential {
            user: PortalUser {
                id: "student".into(),
                username: "student".into(),
                enabled: true,
                session_version: 1,
            },
            password_hash: "hash".into(),
        })
        .await
        .unwrap();
    for key in ["one", "two"] {
        sqlx::query("insert into client_api_keys(id,name,key,created_at,updated_at) values($1,$1,$2,now(),now())").bind(key).bind(format!("sk_{}",key.repeat(43))).execute(&database.pool).await.unwrap();
        store.assign_key(key, "student").await.unwrap();
    }
    store
        .set_wallet_policy(
            "student",
            WalletPolicy {
                balance_enforced: true,
                daily_limit_usd: "0".into(),
                weekly_limit_usd: "0".into(),
                max_concurrency: 1,
            },
        )
        .await
        .unwrap();
    store
        .credit_wallet("student", "credit-a", "1", "test")
        .await
        .unwrap();
    store
        .credit_wallet("student", "credit-a", "1", "test")
        .await
        .unwrap();
    assert!(
        store
            .credit_wallet("student", "credit-a", "2", "test")
            .await
            .is_err()
    );
    let admission = PgPortalAdmission::new(database.pool.clone(), Arc::new(AllowAdmission));
    let request = |key: &str, id: &str| ClientAdmissionRequest {
        model_request_id: ModelRequestId::new(id).unwrap(),
        client_api_key_id: ClientApiKeyId::new(key).unwrap(),
        lease_ttl: StdDuration::from_secs(60),
        limits: Default::default(),
    };
    let one = request("one", "req_one");
    let two = request("two", "req_two");
    assert_eq!(
        admission.admit(one.clone()).await.unwrap(),
        ClientAdmissionDecision::Granted
    );
    assert_eq!(
        admission.admit(two.clone()).await.unwrap(),
        ClientAdmissionDecision::Rejected(ClientAdmissionRejection::ConcurrencyLimited)
    );
    admission
        .release(&one.client_api_key_id, &one.model_request_id)
        .await
        .unwrap();
    assert_eq!(
        admission.admit(two.clone()).await.unwrap(),
        ClientAdmissionDecision::Granted
    );
    let budget = PgClientBudgetStore::new(database.pool.clone());
    let charge = ClientBudgetCharge {
        key_id: two.client_api_key_id.clone(),
        request_id: two.model_request_id.clone(),
        amount_usd: "1".parse().unwrap(),
        completed_at: SystemTime::now(),
    };
    budget.settle(charge.clone()).await.unwrap();
    budget.settle(charge).await.unwrap();
    let wallet = store.wallet("student").await.unwrap();
    assert_eq!(wallet.balance_usd.parse::<f64>().unwrap(), 0.0);
    assert_eq!(wallet.total_spent_usd.parse::<f64>().unwrap(), 1.0);
    assert_eq!(store.wallet_events("student").await.unwrap().len(), 2);
    admission
        .release(&two.client_api_key_id, &two.model_request_id)
        .await
        .unwrap();
    assert_eq!(
        admission.admit(request("one", "req_three")).await.unwrap(),
        ClientAdmissionDecision::Rejected(ClientAdmissionRejection::RateLimited)
    );
    assert!(
        store
            .change_password("student", 1, "new-hash")
            .await
            .unwrap()
    );
    assert!(
        !store
            .change_password("student", 1, "stale-hash")
            .await
            .unwrap()
    );
    database.close().await;
}

#[tokio::test]
async fn portal_sessions_are_invalidated_by_account_changes() {
    let Some(database) = TestDatabase::create("portal_sessions").await else {
        return;
    };
    let store = PgPortalStore::new(database.pool.clone());
    let user = PortalUser {
        id: "student-a".into(),
        username: "alice".into(),
        enabled: true,
        session_version: 1,
    };
    store
        .create_user(PortalCredential {
            user: user.clone(),
            password_hash: "test-hash".into(),
        })
        .await
        .unwrap();
    let session = PortalSession {
        user_id: user.id.clone(),
        session_version: 1,
        expires_at: Utc::now() + Duration::hours(1),
    };
    store
        .store_session("test-session-hash", session.clone())
        .await
        .unwrap();
    assert!(
        store
            .session("test-session-hash")
            .await
            .unwrap()
            .unwrap()
            .authorizes(&store.user("student-a").await.unwrap().unwrap(), Utc::now())
    );
    assert!(store.user("student-b").await.unwrap().is_none());
    store.update_user("student-a", false, None).await.unwrap();
    assert!(!session.authorizes(&store.user("student-a").await.unwrap().unwrap(), Utc::now()));
    store
        .update_user("student-a", true, Some("new-test-hash"))
        .await
        .unwrap();
    assert!(!session.authorizes(&store.user("student-a").await.unwrap().unwrap(), Utc::now()));
    assert_eq!(
        store
            .user_credentials("alice")
            .await
            .unwrap()
            .unwrap()
            .password_hash,
        "new-test-hash"
    );
    store.delete_session("test-session-hash").await.unwrap();
    assert!(store.session("test-session-hash").await.unwrap().is_none());
    database.close().await;
}

#[tokio::test]
async fn portal_keys_are_isolated_and_cannot_transfer_history() {
    let Some(database) = TestDatabase::create("portal_key_isolation").await else {
        return;
    };
    let store = PgPortalStore::new(database.pool.clone());
    for id in ["alice", "bob"] {
        store
            .create_user(PortalCredential {
                user: PortalUser {
                    id: id.into(),
                    username: id.into(),
                    enabled: true,
                    session_version: 1,
                },
                password_hash: "test-hash".into(),
            })
            .await
            .unwrap();
        sqlx::query("insert into client_api_keys(id,name,key,created_at,updated_at) values($1,$1,$2,now(),now())")
            .bind(id).bind(format!("sk_{}", id.repeat(43))).execute(&database.pool).await.unwrap();
        store.assign_key(id, id).await.unwrap();
        store.assign_key(id, id).await.unwrap();
    }
    assert!(store.assign_key("alice", "bob").await.is_err());
    for id in ["alice", "bob"] {
        let keys = store.own_keys(id).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, id);
        assert_eq!(store.owned_key_ids(id).await.unwrap(), vec![id.to_owned()]);
    }
    store.update_user("alice", false, None).await.unwrap();
    assert!(store.own_keys("alice").await.unwrap().is_empty());
    assert_eq!(store.own_keys("bob").await.unwrap().len(), 1);
    database.close().await;
}
