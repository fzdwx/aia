use agent_prompts::AUTO_COMPRESSION_THRESHOLD;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::{
    StreamExt,
    wrappers::{BroadcastStream, errors::BroadcastStreamRecvError},
};

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

fn map_broadcast_result(
    result: Result<SsePayload, BroadcastStreamRecvError>,
) -> Option<Result<Event, Infallible>> {
    match result {
        Ok(payload) => Some(payload.into_axum_event()),
        Err(BroadcastStreamRecvError::Lagged(skipped_messages)) => Some(
            SsePayload::SyncRequired { reason: "lagged".into(), skipped_messages }
                .into_axum_event(),
        ),
    }
}

pub(crate) async fn events(State(state): State<SharedState>) -> impl IntoResponse {
    let rx = state.broadcast_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(map_broadcast_result);

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

#[cfg(test)]
mod tests {
    use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

    use super::map_broadcast_result;

    #[test]
    fn lagged_broadcast_result_maps_to_sync_required_event() {
        let mapped = map_broadcast_result(Err(BroadcastStreamRecvError::Lagged(5)));
        assert!(mapped.is_some());
    }
}
