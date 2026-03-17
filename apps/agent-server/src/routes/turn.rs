use agent_prompts::AUTO_COMPRESSION_THRESHOLD;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{KeepAlive, Sse},
    },
};
use serde::Deserialize;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use crate::{
    sse::{SsePayload, TurnStatus},
    state::SharedState,
};

use super::common::{require_session_id, runtime_worker_error_response};

#[derive(Deserialize)]
pub(crate) struct TurnRequest {
    pub prompt: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CancelTurnRequest {
    pub session_id: Option<String>,
}

pub(crate) async fn events(State(state): State<SharedState>) -> impl IntoResponse {
    let rx = state.broadcast_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(payload) => Some(payload.into_axum_event()),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

pub(crate) async fn submit_turn(
    State(state): State<SharedState>,
    Json(body): Json<TurnRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.get_session_info(session_id.clone()).await {
        Ok(stats) => {
            if stats.pressure_ratio.is_some_and(|ratio| ratio >= AUTO_COMPRESSION_THRESHOLD)
                && let Err(error) =
                    state.session_manager.auto_compress_session(session_id.clone()).await
            {
                return runtime_worker_error_response(error);
            }
        }
        Err(error) => return runtime_worker_error_response(error),
    }

    let _ = state
        .broadcast_tx
        .send(SsePayload::Status { session_id: session_id.clone(), status: TurnStatus::Waiting });

    match state.session_manager.submit_turn(session_id, body.prompt) {
        Ok(()) => (StatusCode::ACCEPTED, Json(serde_json::json!({ "ok": true }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn cancel_turn(
    State(state): State<SharedState>,
    Json(body): Json<CancelTurnRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.cancel_turn(session_id).await {
        Ok(cancelled) => (StatusCode::OK, Json(serde_json::json!({ "cancelled": cancelled }))),
        Err(error) => runtime_worker_error_response(error),
    }
}
