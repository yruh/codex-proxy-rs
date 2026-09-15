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

#[tokio::test]
async fn portal_deletion_revokes_access_and_preserves_history() {
    use gateway_admin::model::portal::PortalKey;
    let Some(database) = TestDatabase::create("portal_deletion").await else {
        return;
    };
    let store = PgPortalStore::new(database.pool.clone());
    let owner = PortalUser {
        id: "owner".into(),
        username: "owner".into(),
        enabled: true,
        session_version: 1,
    };
    let other = PortalUser {
        id: "other".into(),
        username: "other".into(),
        enabled: true,
        session_version: 1,
    };
    for user in [&owner, &other] {
        store
            .create_user(PortalCredential {
                user: user.clone(),
                password_hash: "hash".into(),
            })
            .await
            .unwrap();
    }
    let key = PortalKey {
        id: "delete_key".into(),
        name: "Delete me".into(),
        key: "sk_delete_test".into(),
        enabled: true,
    };
    store.create_own_key(&owner, &key).await.unwrap();
    store
        .credit_wallet(&owner.id, "credit", "10", "retain ledger")
        .await
        .unwrap();
    assert!(store.delete_own_key(&other, &key.id).await.is_err());
    assert_eq!(store.own_keys(&owner.id).await.unwrap().len(), 1);
    store.delete_own_key(&owner, &key.id).await.unwrap();
    assert!(store.own_keys(&owner.id).await.unwrap().is_empty());
    assert_eq!(
        store.owned_key_ids(&owner.id).await.unwrap(),
        vec![key.id.clone()]
    );
    let second = PortalKey {
        id: "second_key".into(),
        key: "sk_second_test".into(),
        ..key
    };
    store.create_own_key(&owner, &second).await.unwrap();
    store.delete_user(&owner.id).await.unwrap();
    assert!(store.user(&owner.id).await.unwrap().is_none());
    assert!(
        store
            .user_credentials(&owner.username)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(store.users().await.unwrap().len(), 1);
    assert!(store.update_user(&owner.id, true, None).await.is_err());
    assert!(store.create_own_key(&owner, &second).await.is_err());
    let count: i64 =
        sqlx::query_scalar("select count(*) from client_api_keys where id='second_key'")
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert_eq!(store.wallet_events(&owner.id).await.unwrap().len(), 1);
    // 重建同名用户不会继承已删除身份的钱包和用量。
    let replacement = PortalUser {
        id: "replacement".into(),
        ..owner
    };
    store
        .create_user(PortalCredential {
            user: replacement.clone(),
            password_hash: "hash".into(),
        })
        .await
        .unwrap();
    assert!(
        store
            .owned_key_ids(&replacement.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .wallet_events(&replacement.id)
            .await
            .unwrap()
            .is_empty()
    );
    database.close().await;
}

#[tokio::test]
async fn own_key_is_bound_atomically_and_pricing_is_fixed_at_admission() {
    use gateway_admin::model::portal::{PortalKey, PortalPricing};
    use gateway_core::engine::budget::{ClientBudgetCharge, ClientBudgetPort};
    use gateway_store::postgres::{PgClientBudgetStore, PgPortalAdmission};
    use std::{sync::Arc, time::SystemTime};
    let Some(database) = TestDatabase::create("portal_pricing").await else {
        return;
    };
    let store = PgPortalStore::new(database.pool.clone());
    let user = PortalUser {
        id: "priced-user".into(),
        username: "priced-user".into(),
        enabled: true,
        session_version: 1,
    };
    store
        .create_user(PortalCredential {
            user: user.clone(),
            password_hash: "hash".into(),
        })
        .await
        .unwrap();
    let key = PortalKey {
        id: "key_priced".into(),
        name: "My computer".into(),
        key: format!("sk_{}", "a".repeat(43)),
        enabled: true,
    };
    store.create_own_key(&user, &key).await.unwrap();
    assert_eq!(
        store.own_keys(&user.id).await.unwrap()[0].name,
        "My computer"
    );
    assert!(store.own_keys("another-user").await.unwrap().is_empty());
    let mut stale = user.clone();
    stale.session_version = 2;
    let invalid = PortalKey {
        id: "key_stale".into(),
        name: "stale".into(),
        key: format!("sk_{}", "b".repeat(43)),
        enabled: true,
    };
    assert!(store.create_own_key(&stale, &invalid).await.is_err());
    let leaked: i64 =
        sqlx::query_scalar("select count(*) from client_api_keys where id='key_stale'")
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(leaked, 0);
    store
        .credit_wallet(&user.id, "test-credit", "10", "")
        .await
        .unwrap();
    store
        .set_pricing(PortalPricing {
            global_multiplier: "1.5".into(),
            model_multipliers: [("discount-model".into(), "0.8".into())].into(),
        })
        .await
        .unwrap();
    let admission = PgPortalAdmission::new(database.pool.clone(), Arc::new(AllowAdmission));
    for id in ["req_discount", "req_global"] {
        let decision = admission
            .admit(ClientAdmissionRequest {
                model_request_id: ModelRequestId::new(id).unwrap(),
                client_api_key_id: ClientApiKeyId::new(&key.id).unwrap(),
                lease_ttl: std::time::Duration::from_secs(60),
                limits: Default::default(),
            })
            .await
            .unwrap();
        assert_eq!(decision, ClientAdmissionDecision::Granted);
    }
    // 改价发生在结算前；已有请求仍用原倍率，模型覆盖不与全局叠乘。
    store
        .set_pricing(PortalPricing {
            global_multiplier: "9".into(),
            model_multipliers: Default::default(),
        })
        .await
        .unwrap();
    // 已准入请求在用户和密钥删除后仍须按原归属完成扣费，不能漏账。
    store.delete_user(&user.id).await.unwrap();
    let budget = PgClientBudgetStore::new(database.pool.clone());
    for (id, model) in [
        ("req_discount", "discount-model"),
        ("req_global", "other-model"),
    ] {
        let charge = ClientBudgetCharge {
            key_id: ClientApiKeyId::new(&key.id).unwrap(),
            request_id: ModelRequestId::new(id).unwrap(),
            model_id: model.into(),
            amount_usd: "1".parse().unwrap(),
            completed_at: SystemTime::now(),
        };
        budget.settle(charge.clone()).await.unwrap();
        budget.settle(charge).await.unwrap();
    }
    assert_eq!(
        store
            .wallet(&user.id)
            .await
            .unwrap()
            .balance_usd
            .parse::<f64>()
            .unwrap(),
        7.7
    );
    let original: String = sqlx::query_scalar(
        "select sum(base_cost_usd)::text from portal_wallet_events where kind='usage'",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(original.parse::<f64>().unwrap(), 2.0);
    let rates: Vec<String> = sqlx::query_scalar(
        "select multiplier::text from portal_wallet_events where kind='usage' order by id",
    )
    .fetch_all(&database.pool)
    .await
    .unwrap();
    assert_eq!(
        rates
            .iter()
            .map(|v| v.parse::<f64>().unwrap())
            .collect::<Vec<_>>(),
        vec![0.8, 1.5]
    );
    database.close().await;
}
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
    let admission = PgPortalAdmission::new(database.pool.clone(), Arc::new(AllowAdmission));
    let request = |key: &str, id: &str| ClientAdmissionRequest {
        model_request_id: ModelRequestId::new(id).unwrap(),
        client_api_key_id: ClientApiKeyId::new(key).unwrap(),
        lease_ttl: StdDuration::from_secs(60),
        limits: Default::default(),
    };
    let one = request("one", "req_one");
    let two = request("two", "req_two");
    // 未充值用户即使未设置任何金额上限，也不能开始调用。
    assert_eq!(
        admission.admit(one.clone()).await.unwrap(),
        ClientAdmissionDecision::Rejected(ClientAdmissionRejection::RateLimited)
    );
    store
        .set_wallet_policy(
            "student",
            WalletPolicy {
                daily_limit_usd: "0".into(),
                weekly_limit_usd: "0".into(),
                max_concurrency: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        admission.admit(two.clone()).await.unwrap(),
        ClientAdmissionDecision::Rejected(ClientAdmissionRejection::RateLimited)
    );
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
        model_id: "test-model".to_owned(),
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
    store
        .credit_wallet("student", "credit-b", "2", "refill")
        .await
        .unwrap();
    assert_eq!(
        admission.admit(request("one", "req_four")).await.unwrap(),
        ClientAdmissionDecision::Granted
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
