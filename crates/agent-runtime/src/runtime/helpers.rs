use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agent_core::{LlmTraceRequestContext, Message, Role, ToolCall};
use session_tape::Anchor;

use crate::{ToolInvocationOutcome, ToolTraceContext};

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

pub(super) fn duration_since_unix_epoch(now: SystemTime) -> Duration {
    now.duration_since(UNIX_EPOCH).unwrap_or_default()
}

pub(super) fn next_turn_id() -> String {
    static NEXT_TURN_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_TURN_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = duration_since_unix_epoch(SystemTime::now()).as_millis();
    format!("turn-{now_ms}-{id}")
}

pub(super) fn now_timestamp_ms() -> u64 {
    duration_since_unix_epoch(SystemTime::now()).as_millis() as u64
}

pub(super) fn anchor_state_message(anchor: &Anchor) -> Option<Message> {
    let summary = anchor.state.get("summary").and_then(|v| v.as_str()).filter(|s| !s.is_empty())?;
    Some(Message::new(Role::User, format!("[context summary]\n{summary}")))
}

pub(super) fn build_llm_trace_context(
    turn_id: &str,
    run_id: &str,
    request_kind: &str,
    step_index: u32,
) -> LlmTraceRequestContext {
    let trace_id = format!("aia-trace-{run_id}");
    let root_span_id = format!("aia-span-{run_id}-root");
    let span_id = format!("aia-span-{run_id}-{request_kind}-{step_index}");
    let operation_name = match request_kind {
        "compression" => "summarize",
        _ => "chat",
    }
    .to_string();

    LlmTraceRequestContext {
        trace_id,
        span_id,
        parent_span_id: Some(root_span_id.clone()),
        root_span_id,
        operation_name,
        turn_id: turn_id.to_string(),
        run_id: run_id.to_string(),
        request_kind: request_kind.to_string(),
        step_index,
    }
}

pub(super) fn build_tool_trace_context(
    parent: &LlmTraceRequestContext,
    call: &ToolCall,
) -> ToolTraceContext {
    ToolTraceContext {
        trace_id: parent.trace_id.clone(),
        span_id: format!("aia-span-{}-tool-{}", parent.run_id, call.invocation_id),
        parent_span_id: parent.span_id.clone(),
        root_span_id: parent.root_span_id.clone(),
        operation_name: "execute_tool".to_string(),
        parent_request_kind: parent.request_kind.clone(),
        parent_step_index: parent.step_index,
    }
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
