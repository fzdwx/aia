use agent_core::StreamEvent;
use agent_runtime::TurnLifecycle;
use axum::response::sse::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
}

#[derive(Clone)]
pub enum SsePayload {
    Stream(StreamEvent),
    Status(TurnStatus),
    TurnCompleted(TurnLifecycle),
    ContextCompressed { summary: String },
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

#[derive(Serialize)]
struct ContextCompressedData {
    summary: String,
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
            Self::ContextCompressed { summary } => {
                let data =
                    serde_json::to_string(&ContextCompressedData { summary }).unwrap_or_default();
                Ok(Event::default().event("context_compressed").data(data))
            }
            Self::Error(message) => {
                let data = serde_json::to_string(&ErrorData { message }).unwrap_or_default();
                Ok(Event::default().event("error").data(data))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use agent_core::StreamEvent;
    use agent_runtime::TurnLifecycle;

    use super::{SsePayload, TurnStatus};

    #[test]
    fn status_payload_can_convert_to_event() {
        let event = SsePayload::Status(TurnStatus::Thinking).into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn turn_completed_payload_can_convert_to_event() {
        let turn = TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "# 用户".into(),
            blocks: vec![agent_runtime::TurnBlock::Assistant { content: "# 回答".into() }],
            assistant_message: Some("# 回答".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let event = SsePayload::TurnCompleted(turn).into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn stream_payload_can_convert_to_event() {
        let event =
            SsePayload::Stream(StreamEvent::TextDelta { text: "增量".into() }).into_axum_event();
        assert!(event.is_ok());
    }
}
