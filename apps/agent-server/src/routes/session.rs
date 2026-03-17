use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::state::SharedState;

use super::common::{
    JsonResponse, json_response, require_session_id, resolve_session_id,
    runtime_worker_error_response, session_resolution_error_response,
};

#[derive(Deserialize)]
pub(crate) struct CreateSessionRequest {
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct HandoffRequest {
    pub name: String,
    pub summary: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AutoCompressRequest {
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionQuery {
    pub session_id: Option<String>,
    pub before_turn_id: Option<String>,
    pub limit: Option<usize>,
}

pub(crate) async fn list_sessions(State(state): State<SharedState>) -> JsonResponse {
    match state.store.list_sessions_async().await {
        Ok(sessions) => json_response(StatusCode::OK, sessions),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

pub(crate) async fn create_session(
    State(state): State<SharedState>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    match state.session_manager.create_session(body.title).await {
        Ok(record) => json_response(StatusCode::CREATED, record),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn delete_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.session_manager.delete_session(id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_session_info(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.get_session_info(session_id).await {
        Ok(stats) => json_response(StatusCode::OK, stats),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn create_handoff(
    State(state): State<SharedState>,
    Json(body): Json<HandoffRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.create_handoff(session_id, body.name, body.summary).await {
        Ok(anchor_entry_id) => {
            (StatusCode::OK, Json(serde_json::json!({ "anchor_entry_id": anchor_entry_id })))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn auto_compress_session(
    State(state): State<SharedState>,
    Json(body): Json<AutoCompressRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.auto_compress_session(session_id).await {
        Ok(compressed) => (StatusCode::OK, Json(serde_json::json!({ "compressed": compressed }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_history(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> JsonResponse {
    let session_id = match resolve_session_id(state.as_ref(), query.session_id).await {
        Ok(Some(session_id)) => session_id,
        Ok(None) => return json_response(StatusCode::OK, Vec::<()>::new()),
        Err(error) => return session_resolution_error_response(error),
    };

    match state.session_manager.get_history(session_id).await {
        Ok(turns) => {
            let limit = query.limit.unwrap_or(5).clamp(1, 200);
            let end_exclusive = query
                .before_turn_id
                .as_ref()
                .and_then(|turn_id| turns.iter().position(|turn| &turn.turn_id == turn_id))
                .unwrap_or(turns.len());
            let start = end_exclusive.saturating_sub(limit);
            let page = turns[start..end_exclusive].to_vec();
            let has_more = start > 0;
            let next_before_turn_id = page.first().map(|turn| turn.turn_id.clone());

            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "turns": page,
                    "has_more": has_more,
                    "next_before_turn_id": next_before_turn_id,
                }),
            )
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_current_turn(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> JsonResponse {
    let session_id = match resolve_session_id(state.as_ref(), query.session_id).await {
        Ok(Some(session_id)) => session_id,
        Ok(None) => return json_response(StatusCode::OK, Option::<()>::None),
        Err(error) => return session_resolution_error_response(error),
    };

    match state.session_manager.get_current_turn(session_id).await {
        Ok(current) => json_response(StatusCode::OK, current),
        Err(error) => runtime_worker_error_response(error),
    }
}
