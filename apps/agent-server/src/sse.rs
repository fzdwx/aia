use agent_core::StreamEvent;
use agent_runtime::TurnLifecycle;
use axum::response::sse::Event;
use serde::Serialize;

/// Status phases visible to the frontend
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
}

#[derive(Clone)]
pub enum SsePayload {
    /// Stream delta from the model
    Stream(StreamEvent),
    /// Turn status changed
    Status(TurnStatus),
    /// Turn completed with full lifecycle
    TurnCompleted(TurnLifecycle),
    /// Error occurred
    Error(String),
}

#[derive(Serialize)]
struct StatusData {
    status: TurnStatus,
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
            Self::Status(status) => {
                let data = serde_json::to_string(&StatusData { status }).unwrap_or_default();
                Ok(Event::default().event("status").data(data))
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
