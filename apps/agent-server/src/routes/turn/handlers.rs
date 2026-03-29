use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use std::convert::Infallible;
use tokio_stream::{
    StreamExt,
    wrappers::{BroadcastStream, errors::BroadcastStreamRecvError},
};

use crate::{sse::SsePayload, state::SharedState};

use super::{CancelTurnRequest, TurnRequest};
use crate::routes::common::{
    prepare_session_for_turn, require_session_id, runtime_worker_error_response,
};

pub(crate) fn map_broadcast_result(
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
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    if let Err(error) = prepare_session_for_turn(state.as_ref(), &session_id).await {
        return runtime_worker_error_response(error);
    }

    match state.session_manager.submit_turn(session_id, vec![body.prompt]).await {
        Ok(turn_id) => {
            (StatusCode::ACCEPTED, Json(serde_json::json!({ "ok": true, "turn_id": turn_id })))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn cancel_turn(
    State(state): State<SharedState>,
    Json(body): Json<CancelTurnRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.cancel_turn(session_id).await {
        Ok(cancelled) => (StatusCode::OK, Json(serde_json::json!({ "cancelled": cancelled }))),
        Err(error) => runtime_worker_error_response(error),
    }
}
