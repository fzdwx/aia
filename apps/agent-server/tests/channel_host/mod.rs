use std::sync::Arc;

use agent_core::StreamEvent;
use agent_runtime::{TurnLifecycle, TurnOutcome};
use agent_store::AiaStore;
use channel_bridge::{
    ChannelRuntimeEvent, ChannelRuntimeSupervisor, ChannelTransport, ChannelTurnStatus,
};

use crate::{
    session_manager::SessionManagerHandle,
    sse::{SsePayload, TurnStatus},
};

use super::{
    mapping::map_sse_payload,
    runtime::{build_channel_adapter_catalog, build_channel_runtime},
};

#[test]
fn map_status_payload_to_channel_runtime_event() {
    let payload = SsePayload::Status {
        session_id: "s1".into(),
        turn_id: "turn-1".into(),
        status: TurnStatus::Thinking,
    };

    let mapped = map_sse_payload(payload);

    assert!(matches!(
        mapped,
        Some(ChannelRuntimeEvent::Status {
            session_id,
            turn_id,
            status: ChannelTurnStatus::Thinking,
        }) if session_id == "s1" && turn_id == "turn-1"
    ));
}

#[test]
fn map_stream_payload_to_channel_runtime_event() {
    let payload = SsePayload::Stream {
        session_id: "s1".into(),
        turn_id: "turn-1".into(),
        event: StreamEvent::TextDelta { text: "增量".into() },
    };

    let mapped = map_sse_payload(payload);

    assert!(matches!(
        mapped,
        Some(ChannelRuntimeEvent::Stream { session_id, turn_id, .. })
            if session_id == "s1" && turn_id == "turn-1"
    ));
}

#[test]
fn build_channel_runtime_registers_feishu_adapter() {
    let store = Arc::new(AiaStore::in_memory().expect("memory store"));
    let session_manager = SessionManagerHandle::test_handle();
    let broadcast_tx = tokio::sync::broadcast::channel(8).0;
    let catalog = build_channel_adapter_catalog(store, session_manager, broadcast_tx);

    let _runtime: ChannelRuntimeSupervisor = build_channel_runtime(catalog);
}

#[test]
fn build_channel_runtime_registers_weixin_definition() {
    let store = Arc::new(AiaStore::in_memory().expect("memory store"));
    let session_manager = SessionManagerHandle::test_handle();
    let broadcast_tx = tokio::sync::broadcast::channel(8).0;
    let catalog = build_channel_adapter_catalog(store, session_manager, broadcast_tx);

    let transports = catalog
        .definitions()
        .into_iter()
        .map(|definition| definition.transport)
        .collect::<Vec<_>>();

    assert!(transports.contains(&ChannelTransport::Feishu));
    assert!(transports.contains(&ChannelTransport::Weixin));
}

#[test]
fn map_turn_completed_payload_to_channel_runtime_event() {
    let payload = SsePayload::TurnCompleted {
        session_id: "s1".into(),
        turn_id: "turn-1".into(),
        turn: TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_messages: vec!["用户问题".into()],
            blocks: vec![agent_runtime::TurnBlock::Assistant { content: "回答".into() }],
            assistant_message: Some("回答".into()),
            thinking: None,
            tool_invocations: vec![],
            usage: None,
            failure_message: None,
            outcome: TurnOutcome::Succeeded,
        },
    };

    let mapped = map_sse_payload(payload);

    assert!(matches!(
        mapped,
        Some(ChannelRuntimeEvent::TurnCompleted { session_id, turn_id, .. })
            if session_id == "s1" && turn_id == "turn-1"
    ));
}

#[test]
fn ignores_non_channel_sse_payloads() {
    let payload =
        SsePayload::SessionCreated { session_id: "session-1".into(), title: "标题".into() };

    assert!(map_sse_payload(payload).is_none());
}
