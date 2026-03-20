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
    let value: serde_json::Value = serde_json::from_str(&json).expect("fallback json should parse");

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
    let event =
        SsePayload::SyncRequired { reason: "lagged".into(), skipped_messages: 3 }.into_axum_event();
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
