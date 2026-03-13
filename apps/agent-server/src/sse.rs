use agent_core::StreamEvent;
use agent_runtime::TurnLifecycle;
use axum::response::sse::Event;
use serde::Serialize;

pub enum SsePayload {
    Stream(StreamEvent),
    TurnCompleted(TurnLifecycle),
    Error(String),
}

#[derive(Serialize)]
struct ErrorData {
    message: String,
}

impl SsePayload {
    pub fn into_axum_event(self) -> Result<Event, std::convert::Infallible> {
        match self {
            Self::Stream(event) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                Ok(Event::default().event("stream").data(data))
            }
            Self::TurnCompleted(turn) => {
                let data = serde_json::to_string(&turn).unwrap_or_default();
                Ok(Event::default().event("turn_completed").data(data))
            }
            Self::Error(message) => {
                let data = serde_json::to_string(&ErrorData { message }).unwrap_or_default();
                Ok(Event::default().event("error").data(data))
            }
        }
    }
}
