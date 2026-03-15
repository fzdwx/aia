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
    Stream { session_id: String, event: StreamEvent },
    Status { session_id: String, status: TurnStatus },
    TurnCompleted { session_id: String, turn: TurnLifecycle },
    ContextCompressed { session_id: String, summary: String },
    Error { session_id: String, message: String },
    SessionCreated { session_id: String, title: String },
    SessionDeleted { session_id: String },
}

#[derive(Serialize)]
struct StatusData {
    session_id: String,
    status: TurnStatus,
}

#[derive(Serialize)]
struct ErrorData {
    session_id: String,
    message: String,
}

#[derive(Serialize)]
struct ContextCompressedData {
    session_id: String,
    summary: String,
}

#[derive(Serialize)]
struct StreamData {
    session_id: String,
    #[serde(flatten)]
    event: StreamEvent,
}

#[derive(Serialize)]
struct TurnCompletedData {
    session_id: String,
    #[serde(flatten)]
    turn: TurnLifecycle,
}

#[derive(Serialize)]
struct SessionCreatedData {
    session_id: String,
    title: String,
}

#[derive(Serialize)]
struct SessionDeletedData {
    session_id: String,
}

impl SsePayload {
    pub fn into_axum_event(self) -> Result<Event, std::convert::Infallible> {
        match self {
            Self::Stream { session_id, event } => {
                let data =
                    serde_json::to_string(&StreamData { session_id, event }).unwrap_or_default();
                Ok(Event::default().event("stream").data(data))
            }
            Self::Status { session_id, status } => {
                let data =
                    serde_json::to_string(&StatusData { session_id, status }).unwrap_or_default();
                Ok(Event::default().event("status").data(data))
            }
            Self::TurnCompleted { session_id, turn } => {
                let data = serde_json::to_string(&TurnCompletedData { session_id, turn })
                    .unwrap_or_default();
                Ok(Event::default().event("turn_completed").data(data))
            }
            Self::ContextCompressed { session_id, summary } => {
                let data = serde_json::to_string(&ContextCompressedData { session_id, summary })
                    .unwrap_or_default();
                Ok(Event::default().event("context_compressed").data(data))
            }
            Self::Error { session_id, message } => {
                let data =
                    serde_json::to_string(&ErrorData { session_id, message }).unwrap_or_default();
                Ok(Event::default().event("error").data(data))
            }
            Self::SessionCreated { session_id, title } => {
                let data = serde_json::to_string(&SessionCreatedData { session_id, title })
                    .unwrap_or_default();
                Ok(Event::default().event("session_created").data(data))
            }
            Self::SessionDeleted { session_id } => {
                let data =
                    serde_json::to_string(&SessionDeletedData { session_id }).unwrap_or_default();
                Ok(Event::default().event("session_deleted").data(data))
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
        let event = SsePayload::Status { session_id: "s1".into(), status: TurnStatus::Thinking }
            .into_axum_event();
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
            usage: Some(agent_core::CompletionUsage {
                input_tokens: 21,
                output_tokens: 9,
                total_tokens: 30,
                cached_tokens: 0,
            }),
            failure_message: None,
        };

        let event = SsePayload::TurnCompleted { session_id: "s1".into(), turn }.into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn stream_payload_can_convert_to_event() {
        let event = SsePayload::Stream {
            session_id: "s1".into(),
            event: StreamEvent::TextDelta { text: "增量".into() },
        }
        .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn session_created_payload_can_convert_to_event() {
        let event =
            SsePayload::SessionCreated { session_id: "s1".into(), title: "New session".into() }
                .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn session_deleted_payload_can_convert_to_event() {
        let event = SsePayload::SessionDeleted { session_id: "s1".into() }.into_axum_event();
        assert!(event.is_ok());
    }
}
