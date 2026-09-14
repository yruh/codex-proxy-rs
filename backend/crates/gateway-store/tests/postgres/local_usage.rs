use super::TestDatabase;
use chrono::{Duration, Utc};
use gateway_admin::{
    model::{local_usage::LocalUsageRecord, observability::TimeRange},
    ports::local_usage::{LocalUsageStore, UsageSyncDevice},
};
use gateway_store::postgres::PgLocalUsageStore;

#[tokio::test]
async fn local_usage_revisions_tombstones_and_revocation_are_enforced() {
    let Some(database) = TestDatabase::create("local_usage_revisions").await else {
        return;
    };
    let now = Utc::now();
    sqlx::query("insert into provider_accounts(id,provider_kind,name,upstream_user_id,authentication_kind,provider_credentials_json,has_refresh_token,access_token_expires_at,credential_state,credential_observed_at,created_at,updated_at) values('account','openai','test','test','oauth','{}'::jsonb,false,now()+interval '1 day','ready',now(),now(),now())")
        .execute(&database.pool).await.unwrap();
    let store = PgLocalUsageStore::new(database.pool.clone());
    store
        .create_device(
            UsageSyncDevice {
                id: "device".into(),
                name: "test".into(),
                provider_account_id: "account".into(),
                enabled: true,
                last_sync_at: None,
            },
            "hash",
        )
        .await
        .unwrap();
    let mut record = LocalUsageRecord {
        record_id: "record".into(),
        revision: 1,
        occurred_at: now,
        session_id: Some("child".into()),
        parent_session_id: Some("parent".into()),
        model: Some("test-model".into()),
        reasoning_effort: None,
        service_tier: None,
        transport: None,
        input_tokens: 100,
        output_tokens: 10,
        cached_tokens: Some(80),
        reasoning_tokens: None,
        duration_ms: None,
        first_token_ms: None,
        excluded: false,
        estimated_usd: Some("0.123456789012".into()),
    };
    assert_eq!(store.ingest("device", &[record.clone()]).await.unwrap(), 1);
    assert_eq!(store.ingest("device", &[record.clone()]).await.unwrap(), 0);
    let range = TimeRange {
        start: now - Duration::hours(1),
        end: now + Duration::hours(1),
    };
    let rows = store.daily_usage(range, Some("account")).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].requests, 1);
    assert_eq!(rows[0].input, "100");
    assert_eq!(rows[0].cached, "80");
    assert_eq!(rows[0].estimated_usd.as_deref(), Some("0.123456789012"));
    assert!(
        store
            .daily_usage(range, Some("other-account"))
            .await
            .unwrap()
            .is_empty()
    );
    record.revision = 2;
    record.excluded = true;
    assert_eq!(store.ingest("device", &[record.clone()]).await.unwrap(), 1);
    record.revision = 1;
    record.excluded = false;
    assert_eq!(store.ingest("device", &[record.clone()]).await.unwrap(), 0);
    assert!(store.daily_usage(range, None).await.unwrap().is_empty());
    store.set_device_enabled("device", false).await.unwrap();
    assert!(store.device_by_token_hash("hash").await.unwrap().is_none());
    record.revision = 3;
    assert!(store.ingest("device", &[record]).await.is_err());
    let count: i64 = sqlx::query_scalar("select count(*) from model_requests")
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    database.close().await;
}
