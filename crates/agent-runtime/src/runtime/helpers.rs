use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{Message, Role, ToolCall};
use session_tape::Anchor;

use crate::ToolInvocationOutcome;

pub(super) fn build_tool_source_entry_ids(
    assistant_entry_id: Option<u64>,
    tool_call_entry_id: u64,
    tool_result_entry_id: u64,
) -> Vec<u64> {
    let mut ids = Vec::with_capacity(3);
    if let Some(assistant_entry_id) = assistant_entry_id {
        ids.push(assistant_entry_id);
    }
    ids.push(tool_call_entry_id);
    ids.push(tool_result_entry_id);
    ids
}

pub(super) fn tool_call_signature(call: &ToolCall) -> String {
    format!("{}:{}", call.tool_name, call.arguments)
}

fn summarize_for_duplicate_message(content: &str) -> String {
    let mut preview = content.chars().take(160).collect::<String>();
    if content.chars().count() > 160 {
        preview.push('…');
    }
    preview.replace('\n', " ")
}

pub(super) fn next_turn_id() -> String {
    static NEXT_TURN_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_TURN_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间应晚于 UNIX_EPOCH")
        .as_millis();
    format!("turn-{now_ms}-{id}")
}

pub(super) fn now_timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("系统时间应晚于 UNIX_EPOCH").as_millis()
        as u64
}

pub(super) fn anchor_state_message(anchor: &Anchor) -> Message {
    let state = &anchor.state;
    let phase = state.get("phase").and_then(|v| v.as_str()).unwrap_or(&anchor.name);
    let summary = state.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let next_steps = state
        .get("next_steps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("、"))
        .unwrap_or_default();
    let source_entry_ids = state
        .get("source_entry_ids")
        .and_then(|v| v.as_array())
        .map(|arr| format!("{:?}", arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>()))
        .unwrap_or_else(|| "[]".into());
    let owner = state.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    Message::new(
        Role::System,
        format!(
            "当前阶段: {}\n锚点摘要: {}\n下一步: {}\n来源条目: {}\n所有者: {}",
            phase, summary, next_steps, source_entry_ids, owner,
        ),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreviousToolCall {
    pub(super) summary: String,
}

impl PreviousToolCall {
    pub(super) fn from_outcome(outcome: &ToolInvocationOutcome) -> Self {
        let summary = match outcome {
            ToolInvocationOutcome::Succeeded { result } => {
                summarize_for_duplicate_message(&result.content)
            }
            ToolInvocationOutcome::Failed { message } => summarize_for_duplicate_message(message),
        };
        Self { summary }
    }
}
