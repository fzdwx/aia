use agent_store::SessionRecord;
use axum::http::StatusCode;

use super::{json_response, resolve_session_id};
use crate::routes::test_support::test_state;

#[test]
fn json_response_serializes_payload() {
    let (status, body) = json_response(StatusCode::CREATED, serde_json::json!({ "ok": true }));

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body.0, serde_json::json!({ "ok": true }));
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_session_id_prefers_explicit_id() {
    let state = test_state();

    let resolved = resolve_session_id(state.as_ref(), Some("session-explicit".into()))
        .await
        .expect("explicit session id should resolve");

    assert_eq!(resolved.as_deref(), Some("session-explicit"));
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_session_id_falls_back_to_first_stored_session() {
    let state = test_state();
    state
        .store
        .create_session(&SessionRecord {
            id: "session-1".into(),
            title: "First".into(),
            created_at: "2026-03-16T00:00:00Z".into(),
            updated_at: "2026-03-16T00:00:00Z".into(),
            model: "bootstrap".into(),
        })
        .expect("first session should save");
    state
        .store
        .create_session(&SessionRecord {
            id: "session-2".into(),
            title: "Second".into(),
            created_at: "2026-03-16T00:00:01Z".into(),
            updated_at: "2026-03-16T00:00:01Z".into(),
            model: "bootstrap".into(),
        })
        .expect("second session should save");

    let resolved = resolve_session_id(state.as_ref(), None)
        .await
        .expect("fallback session lookup should succeed");

    assert_eq!(resolved.as_deref(), Some("session-1"));
}
