use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use agent_core::{StreamEvent, ToolRegistry};
use agent_runtime::{AgentRuntime, ContextStats};

use crate::{
    model::ServerModel,
    runtime_worker::{
        CurrentTurnBlock, CurrentTurnSnapshot, find_tool_output_mut, live_tool_block,
        normalize_object_value,
    },
    sse::TurnStatus,
};

use super::write_lock;

#[derive(Clone, PartialEq)]
pub(crate) enum CurrentStatusInner {
    Waiting,
    Thinking,
    Working,
    Generating,
    Finishing,
}

impl CurrentStatusInner {
    pub(crate) fn to_turn_status(&self) -> TurnStatus {
        match self {
            Self::Waiting => TurnStatus::Waiting,
            Self::Thinking => TurnStatus::Thinking,
            Self::Working => TurnStatus::Working,
            Self::Generating => TurnStatus::Generating,
            Self::Finishing => TurnStatus::Finishing,
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

static NEXT_SERVER_TURN_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_server_turn_id() -> String {
    format!("srv-turn-{}", NEXT_SERVER_TURN_ID.fetch_add(1, Ordering::Relaxed))
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
        StreamEvent::ToolCallDetected { invocation_id, tool_name, arguments, detected_at_ms } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.arguments = normalize_object_value(arguments);
            } else {
                current.blocks.push(live_tool_block(
                    invocation_id.clone(),
                    tool_name.clone(),
                    normalize_object_value(arguments),
                    String::new(),
                    *detected_at_ms,
                    false,
                ));
            }
        }
        StreamEvent::ToolCallStarted { invocation_id, tool_name, arguments, started_at_ms } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.arguments = normalize_object_value(arguments);
                tool.started_at_ms = Some(
                    tool.started_at_ms
                        .map(|existing| existing.min(*started_at_ms))
                        .unwrap_or(*started_at_ms),
                );
            } else {
                current.blocks.push(live_tool_block(
                    invocation_id.clone(),
                    tool_name.clone(),
                    normalize_object_value(arguments),
                    String::new(),
                    *started_at_ms,
                    true,
                ));
            }
        }
        StreamEvent::ToolOutputDelta { invocation_id, text, .. } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.output.push_str(text);
            } else {
                current.blocks.push(live_tool_block(
                    invocation_id.clone(),
                    String::new(),
                    serde_json::json!({}),
                    text.clone(),
                    now_timestamp_ms(),
                    false,
                ));
            }
        }
        StreamEvent::ToolCallCompleted {
            invocation_id,
            tool_name,
            content,
            details,
            failed,
            finished_at_ms,
        } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.completed = true;
                tool.finished_at_ms = Some(*finished_at_ms);
                tool.result_content = Some(content.clone());
                tool.result_details = details.clone();
                tool.failed = Some(*failed);
            }
        }
        StreamEvent::Log { .. } | StreamEvent::Done => {}
    }
}
