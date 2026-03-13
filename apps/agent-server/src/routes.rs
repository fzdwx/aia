use std::convert::Infallible;

use axum::{
    Json,
    extract::State,
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
};
use serde::{Deserialize, Serialize};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

use agent_runtime::{RuntimeEvent, TurnLifecycle};

use crate::{sse::SsePayload, state::SharedState};

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

pub async fn submit_turn(
    State(state): State<SharedState>,
    Json(body): Json<TurnRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<SsePayload>(256);

    tokio::task::spawn_blocking(move || {
        let stream_tx = tx.clone();
        let result = {
            let mut s = state.lock().expect("lock poisoned");
            s.runtime.handle_turn_streaming(&body.prompt, |event| {
                let _ = stream_tx.blocking_send(SsePayload::Stream(event));
            })
        };

        // After handle_turn_streaming completes (lock released), collect events
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
                    let _ = tx.blocking_send(SsePayload::TurnCompleted(turn));
                }
                // Save session
                if let Err(e) = s.runtime.tape().save_jsonl(&s.session_path) {
                    eprintln!("session save failed: {e}");
                }
            }
            Err(error) => {
                let _ = tx.blocking_send(SsePayload::Error(error.to_string()));
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(|payload| payload.into_axum_event());
    Sse::new(stream)
}
