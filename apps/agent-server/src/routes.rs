use std::convert::Infallible;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use agent_core::StreamEvent;
use agent_runtime::{RuntimeEvent, TurnLifecycle};

use crate::{
    sse::{SsePayload, TurnStatus},
    state::SharedState,
};

#[derive(Deserialize)]
pub struct TurnRequest {
    pub prompt: String,
}

#[derive(Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

#[derive(Serialize)]
struct TurnAccepted {
    ok: bool,
}

pub async fn get_providers(State(state): State<SharedState>) -> Json<ProviderInfo> {
    let s = state.lock().expect("lock poisoned");
    Json(ProviderInfo {
        name: s.provider_name.clone(),
        model: s.model_name.clone(),
        connected: true,
    })
}

pub async fn get_history(State(state): State<SharedState>) -> impl IntoResponse {
    let mut s = state.lock().expect("lock poisoned");
    let sub = s.subscriber;
    let events = s.runtime.collect_events(sub).unwrap_or_default();
    let turns: Vec<TurnLifecycle> = events
        .into_iter()
        .filter_map(|event| match event {
            RuntimeEvent::TurnLifecycle { turn } => Some(turn),
            _ => None,
        })
        .collect();
    Json(turns)
}

/// Global SSE endpoint — client connects once, receives all events.
pub async fn events(
    State(state): State<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = {
        let s = state.lock().expect("lock poisoned");
        s.broadcast_tx.subscribe()
    };

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(payload) => Some(payload.into_axum_event()),
        Err(_) => None, // lagged — skip missed events
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Fire-and-forget turn submission. Events arrive via the global SSE stream.
pub async fn submit_turn(
    State(state): State<SharedState>,
    Json(body): Json<TurnRequest>,
) -> impl IntoResponse {
    let broadcast_tx = {
        let s = state.lock().expect("lock poisoned");
        s.broadcast_tx.clone()
    };

    // Immediately signal "waiting"
    let _ = broadcast_tx.send(SsePayload::Status(TurnStatus::Waiting));

    tokio::task::spawn_blocking(move || {
        let mut current_status = CurrentStatus::Waiting;
        let btx = broadcast_tx.clone();

        let result = {
            let mut s = state.lock().expect("lock poisoned");
            s.runtime.handle_turn_streaming(&body.prompt, |event| {
                // Derive status from event kind
                let new_status = match &event {
                    StreamEvent::ThinkingDelta { .. } => CurrentStatus::Thinking,
                    StreamEvent::TextDelta { .. } => CurrentStatus::Generating,
                    StreamEvent::ToolOutputDelta { .. } => CurrentStatus::Working,
                    _ => current_status.clone(),
                };

                if new_status != current_status {
                    current_status = new_status.clone();
                    let _ = btx.send(SsePayload::Status(new_status.to_turn_status()));
                }

                let _ = btx.send(SsePayload::Stream(event));
            })
        };

        match result {
            Ok(_) => {
                let mut s = state.lock().expect("lock poisoned");
                let sub = s.subscriber;
                let events = s.runtime.collect_events(sub).unwrap_or_default();
                let turn = events.into_iter().find_map(|event| match event {
                    RuntimeEvent::TurnLifecycle { turn } => Some(turn),
                    _ => None,
                });
                if let Some(turn) = turn {
                    let _ = broadcast_tx.send(SsePayload::TurnCompleted(turn));
                }
                if let Err(e) = s.runtime.tape().save_jsonl(&s.session_path) {
                    eprintln!("session save failed: {e}");
                }
            }
            Err(error) => {
                let _ = broadcast_tx.send(SsePayload::Error(error.to_string()));
            }
        }
    });

    (StatusCode::ACCEPTED, Json(TurnAccepted { ok: true }))
}

#[derive(Clone, PartialEq)]
enum CurrentStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
}

impl CurrentStatus {
    fn to_turn_status(&self) -> TurnStatus {
        match self {
            Self::Waiting => TurnStatus::Waiting,
            Self::Thinking => TurnStatus::Thinking,
            Self::Working => TurnStatus::Working,
            Self::Generating => TurnStatus::Generating,
        }
    }
}
