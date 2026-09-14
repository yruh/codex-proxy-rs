use chrono::Utc;
use gateway_admin::model::local_usage::*;

fn record() -> LocalUsageRecord {
    LocalUsageRecord {
        record_id: "request-1".into(),
        revision: 1,
        occurred_at: Utc::now(),
        session_id: None,
        parent_session_id: None,
        model: Some("model".into()),
        reasoning_effort: None,
        service_tier: None,
        transport: None,
        input_tokens: 100,
        output_tokens: 10,
        cached_tokens: None,
        reasoning_tokens: None,
        duration_ms: None,
        first_token_ms: None,
        excluded: false,
        estimated_usd: None,
    }
}

#[test]
fn missing_measurements_stay_missing() {
    let value = record();
    assert!(value.validate().is_ok());
    assert_eq!(value.cached_tokens, None);
    assert_eq!(value.first_token_ms, None);
}

#[test]
fn rejects_invalid_token_subsets_and_overflow() {
    let mut value = record();
    value.cached_tokens = Some(101);
    assert!(value.validate().is_err());
    value.cached_tokens = None;
    value.reasoning_tokens = Some(11);
    assert!(value.validate().is_err());
    value.reasoning_tokens = None;
    value.input_tokens = i64::MAX;
    assert!(value.validate().is_err());
}

#[test]
fn rejects_unbounded_fields_and_old_revision_shape() {
    let mut value = record();
    value.revision = 0;
    assert!(value.validate().is_err());
    value.revision = 1;
    value.session_id = Some("x".repeat(257));
    assert!(value.validate().is_err());
}
