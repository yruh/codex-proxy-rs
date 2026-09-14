use super::TestDatabase;
use chrono::{Duration, Utc};
use gateway_admin::{
    model::portal::{PortalCredential, PortalSession, PortalUser},
    ports::portal::PortalStore,
};
use gateway_store::postgres::PgPortalStore;

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
