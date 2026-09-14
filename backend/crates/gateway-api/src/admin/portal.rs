//! 普通用户使用独立 Cookie，管理入口仍强制 AdminAuth。

use super::{
    AdminAuth, AdminEnvelope, AdminError, AdminJson, AdminSessionState,
    wire::map_admin_service_error,
};
use axum::{
    Router,
    extract::State,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{COOKIE, SET_COOKIE},
    },
    response::IntoResponse,
    routing::{get, post},
};
use gateway_admin::model::portal::PortalUser;
use serde::Deserialize;
use serde_json::{Value, json};

const COOKIE_NAME: &str = "cpr_portal_session";

pub fn router<S>() -> Router<S>
where
    S: AdminSessionState + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/portal/login", post(login::<S>))
        .route("/api/portal/status", get(status::<S>))
        .route("/api/portal/logout", post(logout::<S>))
        .route("/api/portal/password", post(change_password::<S>))
        .route("/api/portal/keys", get(keys::<S>))
        .route("/api/portal/usage", get(usage::<S>))
        .route("/api/portal/wallet", get(own_wallet::<S>))
        .route("/api/admin/portal/wallet", get(wallet::<S>))
        .route("/api/admin/portal/wallet/policy", post(wallet_policy::<S>))
        .route("/api/admin/portal/wallet/credit", post(wallet_credit::<S>))
        .route(
            "/api/admin/portal/users",
            get(users::<S>).post(create_user::<S>),
        )
        .route("/api/admin/portal/users/update", post(update_user::<S>))
        .route("/api/admin/portal/keys/assign", post(assign_key::<S>))
}
fn view(user: PortalUser) -> Value {
    json!({"id":user.id,"username":user.username,"enabled":user.enabled})
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WalletQuery {
    user_id: String,
}
async fn own_wallet<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AdminError> {
    let service = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?;
    let user = service
        .current_user(cookie(&headers))
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(
        json!({"wallet":service.wallet(&user.id).await.map_err(map_admin_service_error)?,"events":service.wallet_events(&user.id).await.map_err(map_admin_service_error)?}),
    ))
}
async fn wallet<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
    super::AdminQuery(query): super::AdminQuery<WalletQuery>,
) -> Result<impl IntoResponse, AdminError> {
    let service = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?;
    Ok(ok(
        json!({"wallet":service.wallet(&query.user_id).await.map_err(map_admin_service_error)?,"events":service.wallet_events(&query.user_id).await.map_err(map_admin_service_error)?}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WalletPolicyBody {
    user_id: String,
    policy: gateway_admin::model::portal::WalletPolicy,
}
async fn wallet_policy<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
    AdminJson(body): AdminJson<WalletPolicyBody>,
) -> Result<impl IntoResponse, AdminError> {
    state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .set_wallet_policy(&body.user_id, body.policy)
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(json!({})))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WalletCredit {
    user_id: String,
    operation_id: String,
    amount: String,
    note: String,
}
async fn wallet_credit<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
    AdminJson(body): AdminJson<WalletCredit>,
) -> Result<impl IntoResponse, AdminError> {
    state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .credit_wallet(&body.user_id, &body.operation_id, &body.amount, &body.note)
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(json!({})))
}
fn cookie(headers: &HeaderMap) -> &str {
    headers
        .get(COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| {
            v.split(';').find_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                (name == COOKIE_NAME).then_some(value)
            })
        })
        .unwrap_or("")
}
fn ok(value: Value) -> axum::Json<AdminEnvelope<Value>> {
    axum::Json(AdminEnvelope::ok(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}

async fn login<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    AdminJson(body): AdminJson<Login>,
) -> Result<impl IntoResponse, AdminError> {
    let (token, user) = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .login(&body.username, body.password)
        .await
        .map_err(map_admin_service_error)?;
    let header = HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={token}; Path=/api/portal; HttpOnly; Secure; SameSite=Strict; Max-Age=86400"
    ))
    .map_err(|_| AdminError::internal())?;
    Ok(([(SET_COOKIE, header)], ok(view(user))))
}
async fn status<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AdminError> {
    let user = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .current_user(cookie(&headers))
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(view(user)))
}
async fn logout<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AdminError> {
    state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .logout(cookie(&headers))
        .await
        .map_err(map_admin_service_error)?;
    Ok((
        [(
            SET_COOKIE,
            HeaderValue::from_static(
                "cpr_portal_session=; Path=/api/portal; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
            ),
        )],
        ok(json!({})),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChangePassword {
    old_password: String,
    new_password: String,
}
async fn change_password<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    headers: HeaderMap,
    AdminJson(body): AdminJson<ChangePassword>,
) -> Result<impl IntoResponse, AdminError> {
    state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .change_password(cookie(&headers), body.old_password, body.new_password)
        .await
        .map_err(map_admin_service_error)?;
    Ok((
        [(
            SET_COOKIE,
            HeaderValue::from_static(
                "cpr_portal_session=; Path=/api/portal; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
            ),
        )],
        ok(json!({})),
    ))
}
async fn keys<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AdminError> {
    let keys = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .keys(cookie(&headers))
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(
        json!({"items":keys.into_iter().map(|k|json!({"id":k.id,"name":k.name,"key":k.key,"enabled":k.enabled})).collect::<Vec<_>>()}),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UsageQuery {
    start_time: chrono::DateTime<chrono::Utc>,
    end_time: chrono::DateTime<chrono::Utc>,
    page: u32,
}
async fn usage<S: AdminSessionState + Send + Sync>(
    State(state): State<S>,
    headers: HeaderMap,
    super::AdminQuery(query): super::AdminQuery<UsageQuery>,
) -> Result<impl IntoResponse, AdminError> {
    let rows = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .usage(
            cookie(&headers),
            gateway_admin::model::observability::TimeRange {
                start: query.start_time,
                end: query.end_time,
            },
            query.page,
        )
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(
        json!({"page":query.page,"pageSize":100,"items":rows.into_iter().map(|r|json!({"id":r.id,"keyId":r.key_id,"model":r.model,"occurredAt":r.occurred_at,"inputTokens":r.input_tokens,"outputTokens":r.output_tokens,"cachedTokens":r.cached_tokens,"estimatedUsd":r.cost})).collect::<Vec<_>>()}),
    ))
}
async fn users<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
) -> Result<impl IntoResponse, AdminError> {
    let users = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .users()
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(
        json!({"items":users.into_iter().map(view).collect::<Vec<_>>()}),
    ))
}
async fn create_user<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
    AdminJson(body): AdminJson<Login>,
) -> Result<impl IntoResponse, AdminError> {
    let user = state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .create_user(&body.username, body.password)
        .await
        .map_err(map_admin_service_error)?;
    Ok((StatusCode::CREATED, ok(view(user))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Update {
    id: String,
    enabled: bool,
    password: Option<String>,
}
async fn update_user<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
    AdminJson(body): AdminJson<Update>,
) -> Result<impl IntoResponse, AdminError> {
    state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .update_user(&body.id, body.enabled, body.password)
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(json!({})))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Assign {
    key_id: String,
    user_id: String,
}
async fn assign_key<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(state): State<S>,
    AdminJson(body): AdminJson<Assign>,
) -> Result<impl IntoResponse, AdminError> {
    state
        .admin_services()
        .portal()
        .map_err(map_admin_service_error)?
        .assign_key(&body.key_id, &body.user_id)
        .await
        .map_err(map_admin_service_error)?;
    Ok(ok(json!({})))
}
