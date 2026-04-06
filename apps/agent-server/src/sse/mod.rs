use agent_core::{StreamEvent, UiWidget};
use agent_runtime::TurnLifecycle;
use agent_store::{SessionAutoRenamePolicy, SessionTitleSource};
use axum::response::sse::Event;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runtime_worker::CurrentTurnSnapshot;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Waiting,
    WaitingForQuestion,
    Thinking,
    Working,
    Generating,
    Retrying,
    Finishing,
    Cancelled,
}

#[derive(Clone)]
pub enum SsePayload {
    Stream {
        session_id: String,
        turn_id: String,
        event: StreamEvent,
        widget: Option<UiWidget>,
    },
    Status {
        session_id: String,
        turn_id: String,
        status: TurnStatus,
    },
    CurrentTurnStarted {
        session_id: String,
        current_turn: CurrentTurnSnapshot,
    },
    TurnCompleted {
        session_id: String,
        turn_id: String,
        turn: TurnLifecycle,
    },
    ContextCompressed {
        session_id: String,
        summary: String,
    },
    SyncRequired {
        reason: String,
        skipped_messages: u64,
    },
    Error {
        session_id: String,
        turn_id: Option<String>,
        message: String,
    },
    SessionCreated {
        session_id: String,
        title: String,
    },
    SessionUpdated {
        session_id: String,
        title: String,
        title_source: SessionTitleSource,
        auto_rename_policy: SessionAutoRenamePolicy,
        updated_at: String,
        last_active_at: String,
        model: String,
    },
    SessionDeleted {
        session_id: String,
    },
    TurnCancelled {
        session_id: String,
        turn_id: String,
    },
    /// 消息入队
    MessageQueued {
        session_id: String,
        message_id: String,
        position: u32,
        content_preview: String,
    },
    /// 消息从队列删除
    MessageDeleted {
        session_id: String,
        message_id: String,
        remaining_count: u32,
    },
    /// Turn 被打断
    TurnInterrupted {
        session_id: String,
        turn_id: Option<String>,
    },
    /// 队列开始处理
    QueueProcessing {
        session_id: String,
        count: u32,
    },
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
    #[serde(skip_serializing_if = "Option::is_none")]
    widget: Option<UiWidget>,
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
struct SessionUpdatedData {
    session_id: String,
    title: String,
    title_source: SessionTitleSource,
    auto_rename_policy: SessionAutoRenamePolicy,
    updated_at: String,
    last_active_at: String,
    model: String,
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

#[derive(Serialize)]
struct MessageQueuedData {
    session_id: String,
    message_id: String,
    position: u32,
    content_preview: String,
}

#[derive(Serialize)]
struct MessageDeletedData {
    session_id: String,
    message_id: String,
    remaining_count: u32,
}

#[derive(Serialize)]
struct TurnInterruptedData {
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_id: Option<String>,
}

#[derive(Serialize)]
struct QueueProcessingData {
    session_id: String,
    count: u32,
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
            Self::Stream { session_id, turn_id, event, widget } => {
                Ok(Event::default().event("stream").data(serialize_sse_data(
                    "stream",
                    &StreamData { session_id, turn_id, event, widget },
                )))
            }
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
            Self::SessionUpdated {
                session_id,
                title,
                title_source,
                auto_rename_policy,
                updated_at,
                last_active_at,
                model,
            } => Ok(Event::default().event("session_updated").data(serialize_sse_data(
                "session_updated",
                &SessionUpdatedData {
                    session_id,
                    title,
                    title_source,
                    auto_rename_policy,
                    updated_at,
                    last_active_at,
                    model,
                },
            ))),
            Self::SessionDeleted { session_id } => Ok(Event::default()
                .event("session_deleted")
                .data(serialize_sse_data("session_deleted", &SessionDeletedData { session_id }))),
            Self::TurnCancelled { session_id, turn_id } => {
                Ok(Event::default().event("turn_cancelled").data(serialize_sse_data(
                    "turn_cancelled",
                    &TurnCancelledData { session_id, turn_id },
                )))
            }
            Self::MessageQueued { session_id, message_id, position, content_preview } => {
                Ok(Event::default().event("message_queued").data(serialize_sse_data(
                    "message_queued",
                    &MessageQueuedData { session_id, message_id, position, content_preview },
                )))
            }
            Self::MessageDeleted { session_id, message_id, remaining_count } => {
                Ok(Event::default().event("message_deleted").data(serialize_sse_data(
                    "message_deleted",
                    &MessageDeletedData { session_id, message_id, remaining_count },
                )))
            }
            Self::TurnInterrupted { session_id, turn_id } => {
                Ok(Event::default().event("turn_interrupted").data(serialize_sse_data(
                    "turn_interrupted",
                    &TurnInterruptedData { session_id, turn_id },
                )))
            }
            Self::QueueProcessing { session_id, count } => {
                Ok(Event::default().event("queue_processing").data(serialize_sse_data(
                    "queue_processing",
                    &QueueProcessingData { session_id, count },
                )))
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/sse/mod.rs"]
mod tests;
