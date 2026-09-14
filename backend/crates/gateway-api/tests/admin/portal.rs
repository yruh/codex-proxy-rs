use super::{AdminTestFixture, AdminTestState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn portal_cookie_never_authorizes_admin_or_sync_management() {
    let fixture = AdminTestFixture::new().await;
    fixture.auth.insert_session("valid-session");
    let app = gateway_api::admin::portal::router::<AdminTestState>()
        .merge(gateway_api::admin::local_usage::router::<AdminTestState>())
        .with_state(fixture.state());
    for (method, path, body) in [
        ("GET", "/api/admin/portal/users", ""),
        (
            "POST",
            "/api/admin/portal/users",
            "{\"username\":\"test\",\"password\":\"test-password\"}",
        ),
        (
            "POST",
            "/api/admin/portal/users/update",
            "{\"id\":\"test\",\"enabled\":false}",
        ),
        (
            "POST",
            "/api/admin/portal/keys/assign",
            "{\"keyId\":\"a\",\"userId\":\"b\"}",
        ),
        ("GET", "/api/admin/sync/devices", ""),
        (
            "POST",
            "/api/admin/sync/devices",
            "{\"name\":\"test\",\"accountId\":\"a\"}",
        ),
        (
            "POST",
            "/api/admin/sync/devices/update",
            "{\"id\":\"test\",\"enabled\":false}",
        ),
        (
            "GET",
            "/api/admin/usage/combined?startTime=2026-09-01T00:00:00Z&endTime=2026-09-02T00:00:00Z",
            "",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("cookie", "cpr_portal_session=valid-session")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
}
