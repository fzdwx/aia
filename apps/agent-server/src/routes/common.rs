use agent_store::AiaStoreError;
use axum::{Json, http::StatusCode};
use channel_bridge::prepare_session_for_turn as prepare_channel_session_for_turn;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{session_manager::RuntimeWorkerError, state::AppState};

pub(crate) type JsonResponse = (StatusCode, Json<Value>);

pub(crate) fn error_response(status: StatusCode, message: impl Into<String>) -> JsonResponse {
    (status, Json(json!({ "error": message.into() })))
}

pub(crate) fn ok_response() -> JsonResponse {
    (StatusCode::OK, Json(json!({ "ok": true })))
}

pub(crate) fn runtime_worker_error_response(error: RuntimeWorkerError) -> JsonResponse {
    error_response(error.status, error.message)
}

pub(crate) fn trace_store_error_response(error: AiaStoreError) -> JsonResponse {
    error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

pub(crate) fn json_response<T: Serialize>(status: StatusCode, payload: T) -> JsonResponse {
    match serde_json::to_value(payload) {
        Ok(value) => (status, Json(value)),
        Err(error) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("response serialization failed: {error}"),
        ),
    }
}

pub(crate) fn session_resolution_error_response(error: RuntimeWorkerError) -> JsonResponse {
    runtime_worker_error_response(error)
}

pub(crate) fn no_session_available_response() -> JsonResponse {
    error_response(StatusCode::BAD_REQUEST, "no session available")
}

pub(crate) async fn require_session_id(
    state: &AppState,
    session_id: Option<String>,
) -> Result<String, JsonResponse> {
    match resolve_session_id(state, session_id).await {
        Ok(Some(session_id)) => Ok(session_id),
        Ok(None) => Err(no_session_available_response()),
        Err(error) => Err(session_resolution_error_response(error)),
    }
}

pub(crate) async fn prepare_session_for_turn(
    state: &AppState,
    session_id: &str,
) -> Result<(), RuntimeWorkerError> {
    prepare_channel_session_for_turn(&state.session_manager, session_id)
        .await
        .map_err(|error| RuntimeWorkerError::internal(error.to_string()))
}

pub(crate) async fn resolve_session_id(
    state: &AppState,
    session_id: Option<String>,
) -> Result<Option<String>, RuntimeWorkerError> {
    if let Some(id) = session_id {
        return Ok(Some(id));
    }

    state
        .store
        .first_session_id_async()
        .await
        .map_err(|error| RuntimeWorkerError::internal(format!("session lookup failed: {error}")))
}

#[cfg(test)]
mod tests {
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
}
