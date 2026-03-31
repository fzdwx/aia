use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{
    ConversationItem, Message, ModelRef, QuestionAnswer, QuestionItem, QuestionKind,
    QuestionOption, QuestionRequest, QuestionResult, QuestionResultStatus, Role, ToolCall,
    ToolResult,
};
use serde_json::json;

use crate::entry::{now_iso8601, serialize_payload};
use crate::{
    SessionProviderBinding, SessionTape, TapeEntry, TapeQuery, default_meta, default_session_path,
};

struct FailingSerialize;

impl serde::Serialize for FailingSerialize {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(serde::ser::Error::custom("boom"))
    }
}

fn temp_file(name: &str) -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
    std::env::temp_dir().join(format!("aia-session-{name}-{suffix}.jsonl"))
}

#[test]
fn serialize_payload_falls_back_to_explicit_error_json() {
    let payload = serialize_payload("message", &FailingSerialize);

    assert_eq!(payload["error"], "failed to serialize message: boom");
}

#[test]
fn provider_binding_event_uses_explicit_error_payload_on_serialize_failure() {
    let entry = TapeEntry::event(
        "provider_binding",
        Some(serialize_payload("provider_binding", &FailingSerialize)),
    );

    assert_eq!(entry.event_name(), Some("provider_binding"));
    assert_eq!(
        entry.event_data().and_then(|value| value.get("error")).and_then(|value| value.as_str()),
        Some("failed to serialize provider_binding: boom")
    );
}

#[test]
fn try_latest_provider_binding_returns_error_for_malformed_latest_binding() {
    let mut tape = SessionTape::new();
    tape.bind_provider(SessionProviderBinding::Provider {
        model_ref: ModelRef::new("older", "gpt-4.1-mini"),
        reasoning_effort: None,
    });
    tape.append_entry(TapeEntry::event(
        "provider_binding",
        Some(serde_json::json!({ "broken": true })),
    ));

    let error = tape
        .try_latest_provider_binding()
        .expect_err("malformed latest binding should fail explicitly");

    assert!(error.to_string().contains("provider_binding"));
}

#[test]
fn try_latest_provider_binding_returns_none_when_binding_absent() {
    let tape = SessionTape::new();

    assert_eq!(
        tape.try_latest_provider_binding().expect("missing binding should be allowed"),
        None
    );
}

#[test]
fn try_pending_question_request_returns_latest_unresolved_request() {
    let mut tape = SessionTape::new();
    let older_request = QuestionRequest {
        request_id: "qreq_old".into(),
        invocation_id: "call_old".into(),
        turn_id: "turn_old".into(),
        questions: vec![QuestionItem {
            id: "database".into(),
            question: "Use which database?".into(),
            kind: QuestionKind::Choice,
            required: true,
            multi_select: false,
            options: vec![QuestionOption {
                id: "sqlite".into(),
                label: "SQLite".into(),
                description: Some("simple".into()),
            }],
            placeholder: None,
            recommended_option_id: Some("sqlite".into()),
            recommendation_reason: Some("single-machine setup".into()),
        }],
    };
    let latest_request = QuestionRequest {
        request_id: "qreq_new".into(),
        invocation_id: "call_new".into(),
        turn_id: "turn_new".into(),
        questions: vec![QuestionItem {
            id: "framework".into(),
            question: "Use which framework?".into(),
            kind: QuestionKind::Choice,
            required: true,
            multi_select: false,
            options: vec![QuestionOption {
                id: "axum".into(),
                label: "Axum".into(),
                description: None,
            }],
            placeholder: None,
            recommended_option_id: None,
            recommendation_reason: None,
        }],
    };

    tape.record_question_requested(&older_request);
    tape.record_question_resolved(&QuestionResult {
        status: QuestionResultStatus::Answered,
        request_id: older_request.request_id.clone(),
        answers: vec![QuestionAnswer {
            question_id: "database".into(),
            selected_option_ids: vec!["sqlite".into()],
            text: None,
        }],
        reason: None,
    });
    tape.record_question_requested(&latest_request);

    assert_eq!(
        tape.try_pending_question_request().expect("pending request should decode"),
        Some(latest_request)
    );
}

#[test]
fn try_pending_question_request_returns_none_when_latest_request_was_resolved() {
    let mut tape = SessionTape::new();
    let request = QuestionRequest {
        request_id: "qreq_done".into(),
        invocation_id: "call_done".into(),
        turn_id: "turn_done".into(),
        questions: vec![QuestionItem {
            id: "confirm".into(),
            question: "Continue?".into(),
            kind: QuestionKind::Confirm,
            required: true,
            multi_select: false,
            options: vec![],
            placeholder: None,
            recommended_option_id: None,
            recommendation_reason: None,
        }],
    };

    tape.record_question_requested(&request);
    tape.record_question_resolved(&QuestionResult {
        status: QuestionResultStatus::Cancelled,
        request_id: request.request_id.clone(),
        answers: Vec::new(),
        reason: None,
    });

    assert_eq!(tape.try_pending_question_request().expect("resolved request should decode"), None);
}

#[test]
fn try_pending_question_request_fails_for_malformed_latest_question_requested() {
    let mut tape = SessionTape::new();
    tape.append_entry(TapeEntry::event(
        "question_requested",
        Some(serde_json::json!({ "broken": true })),
    ));

    let error = tape
        .try_pending_question_request()
        .expect_err("malformed question request should fail explicitly");

    assert!(error.to_string().contains("question_requested"));
}

#[test]
fn 默认会话路径位于项目隐藏目录() {
    assert_eq!(default_session_path(), aia_config::default_session_tape_path());
}

#[test]
fn 会记住最近一次_provider_绑定() {
    let mut tape = SessionTape::new();
    tape.bind_provider(SessionProviderBinding::Bootstrap);
    tape.bind_provider(SessionProviderBinding::Provider {
        model_ref: ModelRef::new("main", "gpt-4.1-mini"),
        reasoning_effort: None,
    });

    assert_eq!(
        tape.latest_provider_binding(),
        Some(SessionProviderBinding::Provider {
            model_ref: ModelRef::new("main", "gpt-4.1-mini"),
            reasoning_effort: None,
        })
    );
}

#[test]
fn 锚点以追加条目形式保留在磁带中() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "第一轮"));
    let anchor =
        tape.anchor("discovery", Some(json!({"summary": "发现完成", "next_steps": ["进入实现"]})));
    tape.append(Message::new(Role::Assistant, "第二轮"));

    assert_eq!(anchor.entry_id, 2);
    assert_eq!(tape.entries().len(), 3);
    assert_eq!(tape.entries()[1].kind, "anchor");
    assert_eq!(tape.anchors().len(), 1);
}

#[test]
fn 锚点之后可以按视图重建消息() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "第一轮"));
    tape.append(Message::new(Role::Assistant, "第一轮回复"));
    let anchor =
        tape.anchor("implement", Some(json!({"summary": "实现开始", "next_steps": ["写代码"]})));
    tape.append(Message::new(Role::User, "第二轮"));

    let view = tape.assemble_view(Some(&anchor));

    assert_eq!(view.origin_anchor.as_ref().map(|value| value.entry_id), Some(anchor.entry_id));
    assert_eq!(view.entries.len(), 1);
    assert_eq!(view.messages.len(), 1);
    assert_eq!(view.messages[0].content, "第二轮");
}

#[test]
fn 交接会创建锚点和事件() {
    let mut tape = SessionTape::new();
    let handoff = tape.handoff("phase-2", json!({"summary": "handoff"}));

    assert_eq!(handoff.anchor.name, "phase-2");
    assert_eq!(tape.entries().len(), 2);
    assert_eq!(tape.entries()[0].kind, "anchor");
    assert_eq!(tape.entries()[1].event_name(), Some("handoff"));
}

#[test]
fn default_view_从最近锚点之后开始() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "before"));
    tape.anchor("phase", Some(json!({"summary": "anchor"})));
    tape.append(Message::new(Role::Assistant, "after"));

    let view = tape.default_view();

    assert_eq!(view.messages.len(), 1);
    assert_eq!(view.messages[0].content, "after");
}

#[test]
fn 可按文本查询磁带() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "hello world"));
    tape.append(Message::new(Role::Assistant, "goodbye"));

    let entries = tape
        .query_entries(TapeQuery { text: Some("world".into()), ..TapeQuery::default() })
        .expect("query should succeed");

    assert_eq!(entries.len(), 1);
}

#[test]
fn jsonl_round_trip_preserves_entries() {
    let path = temp_file("round-trip");
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "hello"));
    tape.save_jsonl(&path).expect("save should succeed");

    let restored = SessionTape::load_jsonl_or_default(&path).expect("load should succeed");
    assert_eq!(restored.entries().len(), 1);

    let _ = fs::remove_file(path);
}

#[test]
fn now_iso8601_returns_non_empty_string() {
    assert!(!now_iso8601().is_empty());
}

#[test]
fn default_meta_is_object() {
    assert!(default_meta().is_object());
}

#[test]
fn tool_entries_can_round_trip_shapes() {
    let call = ToolCall::new("read");
    let result = ToolResult::from_call(&call, "ok");

    let call_entry = TapeEntry::tool_call(&call);
    let result_entry = TapeEntry::tool_result(&result);

    assert_eq!(call_entry.as_tool_call(), Some(call));
    assert_eq!(result_entry.as_tool_result(), Some(result));
}

#[test]
fn message_entries_project_back_to_messages() {
    let message = Message::new(Role::User, "hello");
    let entry = TapeEntry::message(&message);

    assert_eq!(entry.as_message(), Some(message));
}

#[test]
fn conversation_items_project_from_default_view() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "hello"));

    let view = tape.default_view();
    assert_eq!(view.conversation.len(), 1);
    assert!(matches!(view.conversation[0], ConversationItem::Message(_)));
}
