use std::sync::{Arc, RwLock};

use agent_core::{StreamEvent, ToolRegistry};
use agent_runtime::{AgentRuntime, ContextStats};
use serde_json::{Value, json};

use crate::{
    model::ServerModel,
    runtime_worker::{CurrentToolOutput, CurrentTurnBlock, CurrentTurnSnapshot},
    sse::TurnStatus,
};

use super::write_lock;

#[derive(Clone, PartialEq)]
pub(crate) enum CurrentStatusInner {
    Waiting,
    Thinking,
    Working,
    Generating,
}

impl CurrentStatusInner {
    pub(crate) fn to_turn_status(&self) -> TurnStatus {
        match self {
            Self::Waiting => TurnStatus::Waiting,
            Self::Thinking => TurnStatus::Thinking,
            Self::Working => TurnStatus::Working,
            Self::Generating => TurnStatus::Generating,
        }
    }
}

pub(crate) fn now_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u64::MAX as u128) as u64,
        Err(_) => 0,
    }
}

pub(crate) fn update_current_turn_status(
    snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    status: TurnStatus,
) {
    if let Some(current) = write_lock(snapshot).as_mut() {
        current.status = status;
    }
}

pub(crate) fn refresh_context_stats_snapshot(
    snapshot: &Arc<RwLock<ContextStats>>,
    runtime: &AgentRuntime<ServerModel, ToolRegistry>,
) {
    *write_lock(snapshot) = runtime.context_stats();
}

pub(crate) fn update_current_turn_from_stream(
    snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    event: &StreamEvent,
) {
    let mut guard = write_lock(snapshot);
    let Some(current) = guard.as_mut() else {
        return;
    };

    match event {
        StreamEvent::ThinkingDelta { text } => match current.blocks.last_mut() {
            Some(CurrentTurnBlock::Thinking { content }) => content.push_str(text),
            _ => current.blocks.push(CurrentTurnBlock::Thinking { content: text.clone() }),
        },
        StreamEvent::TextDelta { text } => match current.blocks.last_mut() {
            Some(CurrentTurnBlock::Text { content }) => content.push_str(text),
            _ => current.blocks.push(CurrentTurnBlock::Text { content: text.clone() }),
        },
        StreamEvent::ToolCallDetected { .. } => {}
        StreamEvent::ToolCallStarted { invocation_id, tool_name, arguments } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.arguments = object_value(arguments);
                tool.started_at_ms = Some(tool.started_at_ms.unwrap_or_else(now_timestamp_ms));
            } else {
                current.blocks.push(tool_block(
                    invocation_id.clone(),
                    tool_name.clone(),
                    object_value(arguments),
                    String::new(),
                    true,
                ));
            }
        }
        StreamEvent::ToolOutputDelta { invocation_id, text, .. } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.output.push_str(text);
            } else {
                current.blocks.push(tool_block(
                    invocation_id.clone(),
                    String::new(),
                    json!({}),
                    text.clone(),
                    true,
                ));
            }
        }
        StreamEvent::ToolCallCompleted { invocation_id, tool_name, content, details, failed } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.completed = true;
                tool.finished_at_ms = Some(now_timestamp_ms());
                tool.result_content = Some(content.clone());
                tool.result_details = details.clone();
                tool.failed = Some(*failed);
            }
        }
        StreamEvent::Log { .. } | StreamEvent::Done => {}
    }
}

fn find_tool_output_mut<'a>(
    blocks: &'a mut [CurrentTurnBlock],
    invocation_id: &str,
) -> Option<&'a mut CurrentToolOutput> {
    blocks.iter_mut().rev().find_map(|block| match block {
        CurrentTurnBlock::Tool { tool } if tool.invocation_id == invocation_id => Some(tool),
        _ => None,
    })
}

fn tool_block(
    invocation_id: String,
    tool_name: String,
    arguments: Value,
    output: String,
    started: bool,
) -> CurrentTurnBlock {
    let timestamp = now_timestamp_ms();
    CurrentTurnBlock::Tool {
        tool: CurrentToolOutput {
            invocation_id,
            tool_name,
            arguments,
            detected_at_ms: timestamp,
            started_at_ms: started.then_some(timestamp),
            finished_at_ms: None,
            output,
            completed: false,
            result_content: None,
            result_details: None,
            failed: None,
        },
    }
}

fn object_value(value: &Value) -> Value {
    if value.is_object() { value.clone() } else { json!({}) }
}
