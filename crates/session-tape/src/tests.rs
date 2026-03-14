use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{ConversationItem, Message, Role, ToolCall, ToolResult};
use serde_json::json;

use crate::entry::now_iso8601;
use crate::{
    SessionProviderBinding, SessionTape, TapeEntry, TapeQuery, default_meta, default_session_path,
};

fn temp_file(name: &str) -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
    std::env::temp_dir().join(format!("aia-session-{name}-{suffix}.jsonl"))
}

#[test]
fn 默认会话路径位于项目隐藏目录() {
    assert_eq!(default_session_path(), PathBuf::from(".aia/session.jsonl"));
}

#[test]
fn 会记住最近一次_provider_绑定() {
    let mut tape = SessionTape::new();
    tape.bind_provider(SessionProviderBinding::Bootstrap);
    tape.bind_provider(SessionProviderBinding::Provider {
        name: "main".into(),
        model: "gpt-4.1-mini".into(),
        base_url: "https://api.openai.com/v1".into(),
        protocol: "openai-responses".into(),
    });

    assert_eq!(
        tape.latest_provider_binding(),
        Some(SessionProviderBinding::Provider {
            name: "main".into(),
            model: "gpt-4.1-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            protocol: "openai-responses".into(),
        })
    );
}

#[test]
fn 旧版_provider_绑定缺少协议字段时仍可恢复() {
    let binding: SessionProviderBinding = serde_json::from_value(serde_json::json!({
        "Provider": {
            "name": "main",
            "model": "gpt-4.1-mini",
            "base_url": "https://api.openai.com/v1"
        }
    }))
    .expect("旧格式应可反序列化");

    assert_eq!(
        binding,
        SessionProviderBinding::Provider {
            name: "main".into(),
            model: "gpt-4.1-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            protocol: "openai-responses".into(),
        }
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
    tape.append(Message::new(Role::User, "开始"));
    tape.append(Message::new(Role::Assistant, "完成发现"));

    let handoff =
        tape.handoff("handoff", json!({"summary": "移交给实现阶段", "next_steps": ["实现运行时"]}));

    assert_eq!(
        handoff.anchor.state.get("summary").and_then(|v| v.as_str()),
        Some("移交给实现阶段")
    );
    assert!(handoff.event_id > handoff.anchor.entry_id);
    assert_eq!(tape.entries()[2].kind, "anchor");
    assert_eq!(tape.entries()[3].kind, "event");
}

#[test]
fn 最新锚点可从磁带中推导() {
    let mut tape = SessionTape::new();
    let first = tape.anchor("d1", Some(json!({"summary": "第一阶段"})));
    let second = tape.anchor("d2", Some(json!({"summary": "第二阶段"})));

    assert_eq!(tape.latest_anchor(), Some(second));
    assert_ne!(tape.latest_anchor(), Some(first));
}

#[test]
fn 默认视图从最新锚点之后组装() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "第一轮"));
    let _ = tape.anchor("d1", Some(json!({"summary": "阶段一"})));
    tape.append(Message::new(Role::Assistant, "第二轮"));

    let messages = tape.default_messages();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "第二轮");
}

#[test]
fn 工具调用与结果可作为条目保留() {
    let mut tape = SessionTape::new();
    let call = ToolCall::new("search_code").with_argument("query", "session-tape");
    let call_id = tape.append_entry(TapeEntry::tool_call(&call));
    let result_id =
        tape.append_entry(TapeEntry::tool_result(&ToolResult::from_call(&call, "found 3 matches")));

    assert_eq!(call_id, 1);
    assert_eq!(result_id, 2);
    assert_eq!(
        tape.entries()[0].as_tool_call().map(|value| value.tool_name.clone()),
        Some("search_code".into())
    );
    assert_eq!(
        tape.entries()[1].as_tool_result().map(|value| value.content.clone()),
        Some("found 3 matches".into())
    );
    assert_eq!(
        tape.entries()[0].as_tool_call().expect("应有工具调用").invocation_id,
        tape.entries()[1].as_tool_result().expect("应有工具结果").invocation_id,
    );
}

#[test]
fn 工具结果投影到默认视图时保留调用标识() {
    let mut tape = SessionTape::new();
    let call = ToolCall::new("search_code").with_argument("query", "session-tape");
    let _ = tape.append_entry(TapeEntry::tool_result(&ToolResult::from_call(&call, "ok")));

    let messages = tape.default_messages();

    assert_eq!(messages.len(), 1);
    assert!(messages[0].content.contains("工具 search_code #"));
    assert!(messages[0].content.contains(&call.invocation_id));
}

#[test]
fn 默认视图会保留结构化工具调用与结果() {
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "开始"));
    let call =
        ToolCall::new("search").with_invocation_id("call-1").with_argument("query", "runtime");
    let _ = tape.append_entry(TapeEntry::tool_call(&call));
    let _ = tape.append_entry(TapeEntry::tool_result(&ToolResult::from_call(&call, "ok")));

    let view = tape.default_view();

    assert!(
        matches!(&view.conversation[0], ConversationItem::Message(message) if message.content == "开始")
    );
    assert!(matches!(&view.conversation[1], ConversationItem::ToolCall(tool_call)
        if tool_call.tool_name == "search" && tool_call.invocation_id == "call-1"));
    assert!(matches!(&view.conversation[2], ConversationItem::ToolResult(result)
        if result.tool_name == "search"
            && result.invocation_id == "call-1"
            && result.content == "ok"));
}

#[test]
fn 保存并载入新格式_jsonl() {
    let path = temp_file("new-format");
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "你好"));
    tape.append_entry(
        TapeEntry::event("turn_started", Some(json!({"turn_id": "turn-1"}))).with_run_id("turn-1"),
    );

    tape.save_jsonl(&path).expect("保存成功");
    let restored = SessionTape::load_jsonl_or_default(&path).expect("载入成功");

    assert_eq!(restored.entries().len(), 2);
    assert_eq!(restored.entries()[0].as_message().map(|m| m.content.clone()), Some("你好".into()));
    assert_eq!(restored.entries()[1].event_name(), Some("turn_started"));

    let _ = fs::remove_file(path);
}

#[test]
fn 可向_jsonl_追加单条记录() {
    let path = temp_file("append-entry");
    let mut tape = SessionTape::new();
    tape.append_entry(TapeEntry::message(&Message::new(Role::User, "第一条")));
    tape.append_entry(TapeEntry::event("turn_completed", Some(json!({"status": "ok"}))));

    SessionTape::append_jsonl_entry(&path, &tape.entries()[0]).expect("应可追加第一条");
    SessionTape::append_jsonl_entry(&path, &tape.entries()[1]).expect("应可追加第二条");

    let restored = SessionTape::load_jsonl_or_default(&path).expect("应可载入追加后的文件");
    assert_eq!(restored.entries().len(), 2);
    assert_eq!(restored.entries()[0].kind, "message");
    assert_eq!(restored.entries()[1].event_name(), Some("turn_completed"));

    let _ = fs::remove_file(path);
}

#[test]
fn 兼容载入旧格式_jsonl() {
    let path = temp_file("legacy-compat");
    let old_line = r#"{"id":1,"fact":{"Message":{"role":"User","content":"旧文件兼容"}},"date":"2026-03-12T10:00:00Z"}"#;
    fs::write(&path, format!("{old_line}\n")).expect("写入成功");

    let mut tape = SessionTape::load_jsonl_or_default(&path).expect("载入成功");
    tape.append(Message::new(Role::Assistant, "继续运行"));
    tape.save_jsonl(&path).expect("保存成功");

    let contents = fs::read_to_string(&path).expect("读取成功");
    assert!(contents.lines().all(|line| line.contains("\"kind\"")));

    let restored = SessionTape::load_jsonl_or_default(&path).expect("再次载入成功");
    assert_eq!(restored.entries().len(), 2);
    assert_eq!(
        restored.entries()[0].as_message().map(|value| value.content.clone()),
        Some("旧文件兼容".into())
    );
    assert_eq!(restored.entries()[0].date, "2026-03-12T10:00:00Z");
    assert_eq!(
        restored.entries()[1].as_message().map(|value| value.content.clone()),
        Some("继续运行".into())
    );

    let _ = fs::remove_file(path);
}

#[test]
fn 查询可按锚点区间日期类型文本与数量切片() {
    let mut tape = SessionTape::named("session");
    let e1 = TapeEntry {
        id: 0,
        kind: "message".into(),
        payload: serde_json::to_value(&Message::new(Role::User, "alpha start")).unwrap(),
        meta: default_meta(),
        date: "2026-03-10T00:00:00Z".into(),
    };
    tape.append_entry(e1);

    let _ = tape.anchor("phase-a", Some(json!({"summary": "阶段 A"})));
    tape.entries.last_mut().unwrap().date = "2026-03-11T00:00:00Z".into();

    let e3 = TapeEntry {
        id: 0,
        kind: "message".into(),
        payload: serde_json::to_value(&Message::new(Role::Assistant, "alpha implementation note"))
            .unwrap(),
        meta: default_meta(),
        date: "2026-03-12T00:00:00Z".into(),
    };
    tape.append_entry(e3);

    let e4 = TapeEntry {
        id: 0,
        kind: "tool_result".into(),
        payload: serde_json::to_value(&ToolResult {
            invocation_id: "tool-call-1".into(),
            tool_name: "search".into(),
            content: "alpha result".into(),
            response_id: None,
            details: None,
        })
        .unwrap(),
        meta: default_meta(),
        date: "2026-03-13T00:00:00Z".into(),
    };
    tape.append_entry(e4);

    let _ = tape.anchor("phase-b", Some(json!({"summary": "阶段 B"})));
    tape.entries.last_mut().unwrap().date = "2026-03-14T00:00:00Z".into();

    let e6 = TapeEntry {
        id: 0,
        kind: "message".into(),
        payload: serde_json::to_value(&Message::new(Role::User, "omega finish")).unwrap(),
        meta: default_meta(),
        date: "2026-03-15T00:00:00Z".into(),
    };
    tape.append_entry(e6);

    let entries = tape
        .query_entries(
            TapeQuery::new()
                .between_anchor_names("phase-a", "phase-b")
                .with_kind("message")
                .matching_text("alpha")
                .within_dates("2026-03-12T00:00:00Z", "2026-03-13T23:59:59Z")
                .limit(1),
        )
        .expect("查询成功");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 3);
    assert_eq!(
        entries[0].as_message().map(|value| value.content.clone()),
        Some("alpha implementation note".into())
    );
}

#[test]
fn 分叉合并只回写增量条目() {
    let mut tape = SessionTape::named("session");
    tape.append(Message::new(Role::User, "base"));
    let mut fork = tape.fork("session-branch");
    fork.append(Message::new(Role::Assistant, "branch reply"));
    let handoff =
        fork.handoff("handoff", json!({"summary": "分叉交接", "next_steps": ["合并回主线"]}));

    let merged_ids = fork.merge_into(&mut tape).expect("合并成功");

    assert_eq!(merged_ids, vec![2, 3, 4]);
    assert_eq!(tape.entries().len(), 4);
    assert_eq!(
        tape.entries()[0].as_message().map(|value| value.content.clone()),
        Some("base".into())
    );
    assert_eq!(
        tape.entries()[1].as_message().map(|value| value.content.clone()),
        Some("branch reply".into())
    );
    assert_eq!(handoff.event_id, 4);
    assert_eq!(tape.entries()[2].kind, "anchor");
    assert_eq!(tape.entries()[3].kind, "event");
}

#[test]
fn 条目工厂方法生成正确的_kind() {
    let msg = TapeEntry::message(&Message::new(Role::User, "hello"));
    assert_eq!(msg.kind, "message");

    let sys = TapeEntry::system("instructions");
    assert_eq!(sys.kind, "system");

    let anchor = TapeEntry::anchor("test", None);
    assert_eq!(anchor.kind, "anchor");

    let call = TapeEntry::tool_call(&ToolCall::new("search"));
    assert_eq!(call.kind, "tool_call");

    let result = TapeEntry::tool_result(&ToolResult::from_call(&ToolCall::new("search"), "ok"));
    assert_eq!(result.kind, "tool_result");

    let event = TapeEntry::event("started", None);
    assert_eq!(event.kind, "event");

    let error = TapeEntry::error("oops");
    assert_eq!(error.kind, "error");
}

#[test]
fn with_meta_和_with_run_id_正确设置元数据() {
    let entry = TapeEntry::message(&Message::new(Role::User, "hello"))
        .with_run_id("turn-1")
        .with_meta("source_entry_ids", json!([1, 2, 3]));

    assert_eq!(entry.meta.get("run_id").and_then(|v| v.as_str()), Some("turn-1"));
    assert!(entry.meta.get("source_entry_ids").is_some());
}

#[test]
fn now_iso8601_生成合法格式() {
    let ts = now_iso8601();
    assert!(ts.contains('T'));
    assert!(ts.ends_with('Z'));
    assert_eq!(ts.len(), 20);
}
