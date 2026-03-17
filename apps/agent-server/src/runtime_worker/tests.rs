use super::*;

use agent_core::{CompletionUsage, Message, Role, ToolCall, ToolResult};
use agent_runtime::{TurnLifecycle, TurnOutcome};
use session_tape::{SessionTape, TapeEntry};

#[test]
fn rebuild_turn_history_from_tape_restores_completed_turns() {
    let mut tape = SessionTape::new();
    let turn_id = "turn-1";
    let user = Message::new(Role::User, "你好");
    let assistant = Message::new(Role::Assistant, "已完成");
    let call = ToolCall::new("read").with_invocation_id("call-1");
    let result = ToolResult::from_call(&call, "内容");

    tape.append_entry(TapeEntry::message(&user).with_run_id(turn_id));
    tape.append_entry(TapeEntry::thinking("思考中").with_run_id(turn_id));
    tape.append_entry(TapeEntry::tool_call(&call).with_run_id(turn_id));
    tape.append_entry(TapeEntry::tool_result(&result).with_run_id(turn_id));
    tape.append_entry(TapeEntry::message(&assistant).with_run_id(turn_id));
    tape.append_entry(TapeEntry::event("turn_completed", None).with_run_id(turn_id));

    let turns = rebuild_session_snapshots_from_tape(&tape).history;

    assert_eq!(turns.len(), 1);
    let turn = &turns[0];
    assert_eq!(turn.turn_id, turn_id);
    assert_eq!(turn.user_message, "你好");
    assert_eq!(turn.assistant_message.as_deref(), Some("已完成"));
    assert_eq!(turn.thinking.as_deref(), Some("思考中"));
    assert_eq!(turn.tool_invocations.len(), 1);
    assert_eq!(turn.blocks.len(), 3);
}

#[test]
fn rebuild_turn_history_from_tape_restores_turn_usage() {
    let mut tape = SessionTape::new();
    let turn_id = "turn-usage";
    tape.append_entry(
        TapeEntry::message(&Message::new(Role::User, "统计 token")).with_run_id(turn_id),
    );
    tape.append_entry(
        TapeEntry::message(&Message::new(Role::Assistant, "本次调用已完成")).with_run_id(turn_id),
    );
    tape.append_entry(
        TapeEntry::event(
            "turn_completed",
            Some(serde_json::json!({
                "status": "ok",
                "usage": {
                    "input_tokens": 21,
                    "output_tokens": 9,
                    "total_tokens": 30,
                    "cached_tokens": 0
                }
            })),
        )
        .with_run_id(turn_id),
    );

    let turns = rebuild_session_snapshots_from_tape(&tape).history;

    assert_eq!(turns.len(), 1);
    assert_eq!(
        turns[0].usage,
        Some(CompletionUsage {
            input_tokens: 21,
            output_tokens: 9,
            total_tokens: 30,
            cached_tokens: 0,
        })
    );
}

#[test]
fn rebuild_turn_history_from_tape_ignores_invalid_usage_payload_without_dropping_turn() {
    let mut tape = SessionTape::new();
    let turn_id = "turn-bad-usage";
    tape.append_entry(
        TapeEntry::message(&Message::new(Role::User, "统计 token")).with_run_id(turn_id),
    );
    tape.append_entry(
        TapeEntry::message(&Message::new(Role::Assistant, "本次调用已完成")).with_run_id(turn_id),
    );
    tape.append_entry(
        TapeEntry::event(
            "turn_completed",
            Some(serde_json::json!({
                "status": "ok",
                "usage": {
                    "input_tokens": "bad",
                    "output_tokens": 9,
                    "total_tokens": 30,
                    "cached_tokens": 0
                }
            })),
        )
        .with_run_id(turn_id),
    );

    let turns = rebuild_session_snapshots_from_tape(&tape).history;

    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].turn_id, turn_id);
    assert_eq!(turns[0].usage, None);
    assert_eq!(turns[0].outcome, TurnOutcome::Succeeded);
}

#[test]
fn rebuild_session_snapshots_from_tape_keeps_incomplete_turn_out_of_history() {
    let mut tape = SessionTape::new();
    let turn_id = "turn-1";
    tape.append_entry(TapeEntry::message(&Message::new(Role::User, "处理中")).with_run_id(turn_id));
    tape.append_entry(TapeEntry::thinking("先分析").with_run_id(turn_id));

    let snapshots = rebuild_session_snapshots_from_tape(&tape);

    assert!(snapshots.history.is_empty());
    let current = snapshots.current_turn.expect("应保留当前未完成轮次");
    assert_eq!(current.user_message, "处理中");
    assert_eq!(current.status, crate::sse::TurnStatus::Thinking);
    assert!(current.started_at_ms > 0);
    assert_eq!(current.blocks, vec![CurrentTurnBlock::Thinking { content: "先分析".to_string() }]);
}

#[test]
fn rebuild_session_snapshots_from_tape_projects_completed_tool_block() {
    let mut tape = SessionTape::new();
    let turn_id = "turn-tool";
    let call =
        ToolCall::new("read").with_invocation_id("call-1").with_argument("file_path", "Cargo.toml");
    let result =
        ToolResult::from_call(&call, "内容").with_details(serde_json::json!({ "lines": [1, 2] }));

    tape.append_entry(TapeEntry::message(&Message::new(Role::User, "读一下")).with_run_id(turn_id));
    tape.append_entry(TapeEntry::tool_call(&call).with_run_id(turn_id));
    tape.append_entry(TapeEntry::tool_result(&result).with_run_id(turn_id));

    let snapshots = rebuild_session_snapshots_from_tape(&tape);

    let current = snapshots.current_turn.expect("应保留当前未完成轮次");
    assert_eq!(current.status, crate::sse::TurnStatus::Working);
    assert_eq!(
        current.blocks,
        vec![CurrentTurnBlock::Tool {
            tool: CurrentToolOutput {
                invocation_id: "call-1".to_string(),
                tool_name: "read".to_string(),
                arguments: serde_json::json!({ "file_path": "Cargo.toml" }),
                detected_at_ms: current.started_at_ms,
                started_at_ms: Some(current.started_at_ms),
                finished_at_ms: Some(current.started_at_ms),
                output: String::new(),
                completed: true,
                result_content: Some("内容".to_string()),
                result_details: Some(serde_json::json!({ "lines": [1, 2] })),
                failed: Some(false),
            }
        }]
    );
}

#[test]
fn rebuild_turn_history_from_tape_marks_cancelled_blocks_explicitly() {
    let mut tape = SessionTape::new();
    let turn_id = "turn-cancelled";
    tape.append_entry(TapeEntry::message(&Message::new(Role::User, "处理中")).with_run_id(turn_id));
    tape.append_entry(TapeEntry::thinking("先分析").with_run_id(turn_id));
    tape.append_entry(
        TapeEntry::message(&Message::new(Role::Assistant, "部分回答")).with_run_id(turn_id),
    );
    tape.append_entry(
        TapeEntry::event("turn_failed", Some(serde_json::json!({ "message": "本轮已取消" })))
            .with_run_id(turn_id),
    );

    let turns = rebuild_session_snapshots_from_tape(&tape).history;

    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].outcome, TurnOutcome::Cancelled);
    assert!(turns[0].blocks.iter().any(|block| matches!(
        block,
        agent_runtime::TurnBlock::Cancelled { message } if message == "本轮已取消"
    )));
}

#[test]
fn rebuild_turn_history_from_tape_restores_legacy_turn_record() {
    let mut tape = SessionTape::new();
    let legacy_turn = TurnLifecycle {
        turn_id: "legacy-turn-1".to_string(),
        started_at_ms: 1000,
        finished_at_ms: 2000,
        source_entry_ids: vec![1, 2],
        user_message: "旧问题".to_string(),
        blocks: vec![agent_runtime::TurnBlock::Assistant { content: "旧回答".to_string() }],
        assistant_message: Some("旧回答".to_string()),
        thinking: None,
        tool_invocations: vec![],
        usage: None,
        failure_message: None,
        outcome: TurnOutcome::Succeeded,
    };
    tape.append_entry(TapeEntry::event(
        "turn_record",
        Some(serde_json::to_value(&legacy_turn).expect("legacy turn should serialize")),
    ));

    let turns = rebuild_session_snapshots_from_tape(&tape).history;

    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0], legacy_turn);
}
