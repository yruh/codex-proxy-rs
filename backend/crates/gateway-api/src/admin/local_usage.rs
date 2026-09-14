//! 同步写入口使用设备凭据，管理入口仍需管理员身份。

use super::{
    AdminAuth, AdminEnvelope, AdminError, AdminJson, AdminSessionState,
    wire::map_admin_service_error,
};
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, header::AUTHORIZATION},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use gateway_admin::{model::local_usage::LocalUsageRecord, ports::local_usage::UsageSyncDevice};
use serde::Deserialize;
use serde_json::json;

pub fn router<S>() -> Router<S>
where
    S: AdminSessionState + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/admin/usage/combined", get(combined::<S>))
        .route("/api/sync/usage/combined", get(sync_combined::<S>))
        .route("/api/sync/usage", post(ingest::<S>))
        .route(
            "/api/admin/sync/devices",
            get(devices::<S>).post(create::<S>),
        )
        .route("/api/admin/sync/devices/update", post(update::<S>))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RangeQuery {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    account_id: Option<String>,
}
fn daily(rows: Vec<gateway_admin::ports::local_usage::DailySourceUsage>) -> serde_json::Value {
    json!({"items":rows.into_iter().map(|r|json!({"day":r.day,"source":r.source,"requests":r.requests,"inputTokens":r.input,"outputTokens":r.output,"cachedTokens":r.cached,"estimatedUsd":r.estimated_usd,"pricedRequests":r.priced_requests})).collect::<Vec<_>>()})
}
async fn combined<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(s): State<S>,
    super::AdminQuery(q): super::AdminQuery<RangeQuery>,
) -> Result<impl IntoResponse, AdminError> {
    let rows = s
        .admin_services()
        .local_usage()
        .map_err(map_admin_service_error)?
        .daily_usage(
            gateway_admin::model::observability::TimeRange {
                start: q.start_time,
                end: q.end_time,
            },
            q.account_id.as_deref(),
        )
        .await
        .map_err(map_admin_service_error)?;
    Ok(axum::Json(AdminEnvelope::ok(daily(rows))))
}
async fn sync_combined<S: AdminSessionState + Send + Sync>(
    State(s): State<S>,
    headers: HeaderMap,
    super::AdminQuery(q): super::AdminQuery<RangeQuery>,
) -> Result<impl IntoResponse, AdminError> {
    let service = s
        .admin_services()
        .local_usage()
        .map_err(map_admin_service_error)?;
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let d = service
        .authenticate(token)
        .await
        .map_err(map_admin_service_error)?;
    let rows = service
        .daily_usage(
            gateway_admin::model::observability::TimeRange {
                start: q.start_time,
                end: q.end_time,
            },
            Some(&d.provider_account_id),
        )
        .await
        .map_err(map_admin_service_error)?;
    Ok(axum::Json(AdminEnvelope::ok(daily(rows))))
}
fn device(d: UsageSyncDevice) -> serde_json::Value {
    json!({"id":d.id,"name":d.name,"accountId":d.provider_account_id,"enabled":d.enabled,"lastSyncAt":d.last_sync_at})
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Create {
    name: String,
    account_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: String,
    enabled: bool,
}
async fn devices<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(s): State<S>,
) -> Result<impl IntoResponse, AdminError> {
    let rows = s
        .admin_services()
        .local_usage()
        .map_err(map_admin_service_error)?
        .devices()
        .await
        .map_err(map_admin_service_error)?;
    Ok(axum::Json(AdminEnvelope::ok(
        json!({"items":rows.into_iter().map(device).collect::<Vec<_>>()}),
    )))
}
async fn create<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(s): State<S>,
    AdminJson(b): AdminJson<Create>,
) -> Result<impl IntoResponse, AdminError> {
    let (d, token) = s
        .admin_services()
        .local_usage()
        .map_err(map_admin_service_error)?
        .create_device(b.name, b.account_id)
        .await
        .map_err(map_admin_service_error)?;
    Ok(axum::Json(AdminEnvelope::ok(
        json!({"device":device(d),"token":token}),
    )))
}
async fn update<S: AdminSessionState + Send + Sync>(
    _auth: AdminAuth,
    State(s): State<S>,
    AdminJson(b): AdminJson<Update>,
) -> Result<impl IntoResponse, AdminError> {
    s.admin_services()
        .local_usage()
        .map_err(map_admin_service_error)?
        .set_enabled(&b.id, b.enabled)
        .await
        .map_err(map_admin_service_error)?;
    Ok(axum::Json(AdminEnvelope::ok(json!({}))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    record_id: String,
    revision: i64,
    occurred_at: DateTime<Utc>,
    session_id: Option<String>,
    parent_session_id: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    service_tier: Option<String>,
    transport: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    duration_ms: Option<i64>,
    first_token_ms: Option<i64>,
    #[serde(default)]
    excluded: bool,
    estimated_usd: Option<String>,
}
impl From<Record> for LocalUsageRecord {
    fn from(r: Record) -> Self {
        Self {
            record_id: r.record_id,
            revision: r.revision,
            occurred_at: r.occurred_at,
            session_id: r.session_id,
            parent_session_id: r.parent_session_id,
            model: r.model,
            reasoning_effort: r.reasoning_effort,
            service_tier: r.service_tier,
            transport: r.transport,
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            cached_tokens: r.cached_tokens,
            reasoning_tokens: r.reasoning_tokens,
            duration_ms: r.duration_ms,
            first_token_ms: r.first_token_ms,
            excluded: r.excluded,
            estimated_usd: r.estimated_usd,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    records: Vec<Record>,
}
async fn ingest<S: AdminSessionState + Send + Sync>(
    State(s): State<S>,
    headers: HeaderMap,
    AdminJson(b): AdminJson<Batch>,
) -> Result<impl IntoResponse, AdminError> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let rows = b
        .records
        .into_iter()
        .map(LocalUsageRecord::from)
        .collect::<Vec<_>>();
    let changed = s
        .admin_services()
        .local_usage()
        .map_err(map_admin_service_error)?
        .ingest(token, &rows)
        .await
        .map_err(map_admin_service_error)?;
    Ok(axum::Json(AdminEnvelope::ok(
        json!({"accepted":rows.len(),"changed":changed}),
    )))
}
