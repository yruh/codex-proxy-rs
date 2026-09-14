use chrono::Duration;
use chrono::Utc;
use gateway_admin::model::portal::*;

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
