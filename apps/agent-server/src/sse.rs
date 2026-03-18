use agent_core::StreamEvent;
use agent_runtime::TurnLifecycle;
use axum::response::sse::Event;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runtime_worker::CurrentTurnSnapshot;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
    Cancelled,
}

#[derive(Clone)]
pub enum SsePayload {
    Stream { session_id: String, turn_id: String, event: StreamEvent },
    Status { session_id: String, turn_id: String, status: TurnStatus },
    CurrentTurnStarted { session_id: String, current_turn: CurrentTurnSnapshot },
    TurnCompleted { session_id: String, turn_id: String, turn: TurnLifecycle },
    ContextCompressed { session_id: String, summary: String },
    SyncRequired { reason: String, skipped_messages: u64 },
    Error { session_id: String, turn_id: Option<String>, message: String },
    SessionCreated { session_id: String, title: String },
    SessionDeleted { session_id: String },
    TurnCancelled { session_id: String, turn_id: String },
}

#[derive(Serialize)]
struct StatusData {
    session_id: String,
    turn_id: String,
    status: TurnStatus,
}

#[derive(Serialize)]
struct CurrentTurnStartedData {
    session_id: String,
    #[serde(flatten)]
    current_turn: CurrentTurnSnapshot,
}

#[derive(Serialize)]
struct ErrorData {
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_id: Option<String>,
    message: String,
}

#[derive(Serialize)]
struct ContextCompressedData {
    session_id: String,
    summary: String,
}

#[derive(Serialize)]
struct SyncRequiredData {
    reason: String,
    skipped_messages: u64,
}

#[derive(Serialize)]
struct StreamData {
    session_id: String,
    turn_id: String,
    #[serde(flatten)]
    event: StreamEvent,
}

#[derive(Serialize)]
struct TurnCompletedData {
    session_id: String,
    turn_id: String,
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

#[derive(Serialize)]
struct TurnCancelledData {
    session_id: String,
    turn_id: String,
}

fn serialize_sse_data<T: Serialize>(event_name: &str, payload: &T) -> String {
    serde_json::to_string(payload).unwrap_or_else(|error| {
        json!({
            "error": format!("failed to serialize SSE payload for {event_name}: {error}")
        })
        .to_string()
    })
}

impl SsePayload {
    pub fn into_axum_event(self) -> Result<Event, std::convert::Infallible> {
        match self {
            Self::Stream { session_id, turn_id, event } => Ok(Event::default()
                .event("stream")
                .data(serialize_sse_data("stream", &StreamData { session_id, turn_id, event }))),
            Self::Status { session_id, turn_id, status } => Ok(Event::default()
                .event("status")
                .data(serialize_sse_data("status", &StatusData { session_id, turn_id, status }))),
            Self::CurrentTurnStarted { session_id, current_turn } => {
                Ok(Event::default().event("current_turn_started").data(serialize_sse_data(
                    "current_turn_started",
                    &CurrentTurnStartedData { session_id, current_turn },
                )))
            }
            Self::TurnCompleted { session_id, turn_id, turn } => {
                Ok(Event::default().event("turn_completed").data(serialize_sse_data(
                    "turn_completed",
                    &TurnCompletedData { session_id, turn_id, turn },
                )))
            }
            Self::ContextCompressed { session_id, summary } => {
                Ok(Event::default().event("context_compressed").data(serialize_sse_data(
                    "context_compressed",
                    &ContextCompressedData { session_id, summary },
                )))
            }
            Self::SyncRequired { reason, skipped_messages } => {
                Ok(Event::default().event("sync_required").data(serialize_sse_data(
                    "sync_required",
                    &SyncRequiredData { reason, skipped_messages },
                )))
            }
            Self::Error { session_id, turn_id, message } => Ok(Event::default()
                .event("error")
                .data(serialize_sse_data("error", &ErrorData { session_id, turn_id, message }))),
            Self::SessionCreated { session_id, title } => {
                Ok(Event::default().event("session_created").data(serialize_sse_data(
                    "session_created",
                    &SessionCreatedData { session_id, title },
                )))
            }
            Self::SessionDeleted { session_id } => Ok(Event::default()
                .event("session_deleted")
                .data(serialize_sse_data("session_deleted", &SessionDeletedData { session_id }))),
            Self::TurnCancelled { session_id, turn_id } => {
                Ok(Event::default().event("turn_cancelled").data(serialize_sse_data(
                    "turn_cancelled",
                    &TurnCancelledData { session_id, turn_id },
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use agent_core::StreamEvent;
    use agent_runtime::{TurnLifecycle, TurnOutcome};

    use crate::runtime_worker::CurrentTurnSnapshot;

    use super::{SsePayload, TurnStatus, serialize_sse_data};

    struct FailingPayload;

    impl serde::Serialize for FailingPayload {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("boom"))
        }
    }

    #[test]
    fn serialize_sse_data_falls_back_to_error_payload() {
        let json = serialize_sse_data("test_event", &FailingPayload);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("fallback json should parse");

        assert_eq!(value["error"], "failed to serialize SSE payload for test_event: boom");
    }

    #[test]
    fn status_payload_can_convert_to_event() {
        let event = SsePayload::Status {
            session_id: "s1".into(),
            turn_id: "turn-1".into(),
            status: TurnStatus::Thinking,
        }
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
            outcome: TurnOutcome::Succeeded,
        };

        let event =
            SsePayload::TurnCompleted { session_id: "s1".into(), turn_id: "turn-1".into(), turn }
                .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn current_turn_started_payload_can_convert_to_event() {
        let event = SsePayload::CurrentTurnStarted {
            session_id: "s1".into(),
            current_turn: CurrentTurnSnapshot {
                turn_id: "turn-1".into(),
                started_at_ms: 1,
                user_message: "外部消息".into(),
                status: TurnStatus::Waiting,
                blocks: vec![],
            },
        }
        .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn stream_payload_can_convert_to_event() {
        let event = SsePayload::Stream {
            session_id: "s1".into(),
            turn_id: "turn-1".into(),
            event: StreamEvent::TextDelta { text: "增量".into() },
        }
        .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn sync_required_payload_can_convert_to_event() {
        let event = SsePayload::SyncRequired { reason: "lagged".into(), skipped_messages: 3 }
            .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn session_created_payload_can_convert_to_event() {
        let event = SsePayload::SessionCreated {
            session_id: "s1".into(),
            title: aia_config::DEFAULT_SESSION_TITLE.into(),
        }
        .into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn session_deleted_payload_can_convert_to_event() {
        let event = SsePayload::SessionDeleted { session_id: "s1".into() }.into_axum_event();
        assert!(event.is_ok());
    }

    #[test]
    fn turn_cancelled_payload_can_convert_to_event() {
        let event = SsePayload::TurnCancelled { session_id: "s1".into(), turn_id: "turn-1".into() }
            .into_axum_event();
        assert!(event.is_ok());
    }
}
