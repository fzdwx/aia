use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use agent_core::{StreamEvent, ToolOutputStream, ToolRegistry};
use agent_runtime::{AgentRuntime, ContextStats};

use crate::{
    model::ServerModel,
    runtime_worker::{
        CurrentToolOutput, CurrentToolOutputSegment, CurrentTurnBlock, CurrentTurnSnapshot,
        find_tool_output_mut, live_tool_block, normalize_object_value, sync_widget_projection,
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
    Retrying,
    Finishing,
}

impl CurrentStatusInner {
    pub(crate) fn to_turn_status(&self) -> TurnStatus {
        match self {
            Self::Waiting => TurnStatus::Waiting,
            Self::Thinking => TurnStatus::Thinking,
            Self::Working => TurnStatus::Working,
            Self::Generating => TurnStatus::Generating,
            Self::Retrying => TurnStatus::Retrying,
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

const MAX_TOOL_OUTPUT_SEGMENTS: usize = 200;

pub(crate) fn next_server_turn_id() -> String {
    format!("srv-turn-{}", NEXT_SERVER_TURN_ID.fetch_add(1, Ordering::Relaxed))
}

fn append_tool_output_segment(tool: &mut CurrentToolOutput, stream: ToolOutputStream, text: &str) {
    tool.output.push_str(text);

    let segments = tool.output_segments.get_or_insert_with(Vec::new);
    if let Some(last) = segments.last_mut()
        && last.stream == stream
    {
        last.text.push_str(text);
        return;
    }

    if segments.len() >= MAX_TOOL_OUTPUT_SEGMENTS {
        segments.remove(0);
    }

    segments.push(CurrentToolOutputSegment { stream, text: text.to_string() });
}

fn sync_tool_arguments_from_raw(tool: &mut CurrentToolOutput) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&tool.raw_arguments)
        && value.is_object()
    {
        tool.arguments = normalize_object_value(&value);
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
        StreamEvent::ToolCallDetected { invocation_id, tool_name, arguments, detected_at_ms } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.arguments = normalize_object_value(arguments);
                if tool.raw_arguments.is_empty() {
                    tool.raw_arguments =
                        serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                }
                sync_widget_projection(tool);
            } else {
                let mut block = live_tool_block(
                    invocation_id.clone(),
                    tool_name.clone(),
                    normalize_object_value(arguments),
                    String::new(),
                    None,
                    *detected_at_ms,
                    false,
                );
                if let CurrentTurnBlock::Tool { tool } = &mut block {
                    tool.raw_arguments =
                        serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                    sync_widget_projection(tool);
                }
                current.blocks.push(block);
            }
        }
        StreamEvent::ToolCallArgumentsDelta { invocation_id, tool_name, arguments_delta } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                if !tool_name.is_empty() {
                    tool.tool_name = tool_name.clone();
                }
                tool.raw_arguments.push_str(arguments_delta);
                sync_tool_arguments_from_raw(tool);
                sync_widget_projection(tool);
            } else {
                let mut block = live_tool_block(
                    invocation_id.clone(),
                    tool_name.clone(),
                    serde_json::json!({}),
                    String::new(),
                    None,
                    now_timestamp_ms(),
                    false,
                );
                if let CurrentTurnBlock::Tool { tool } = &mut block {
                    tool.raw_arguments = arguments_delta.clone();
                    sync_tool_arguments_from_raw(tool);
                    sync_widget_projection(tool);
                }
                current.blocks.push(block);
            }
        }
        StreamEvent::ToolCallReady { call } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, &call.invocation_id) {
                tool.tool_name = call.tool_name.clone();
                tool.arguments = normalize_object_value(&call.arguments);
                if tool.raw_arguments.is_empty() {
                    tool.raw_arguments =
                        serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
                }
                sync_widget_projection(tool);
            } else {
                let mut block = live_tool_block(
                    call.invocation_id.clone(),
                    call.tool_name.clone(),
                    normalize_object_value(&call.arguments),
                    String::new(),
                    None,
                    now_timestamp_ms(),
                    false,
                );
                if let CurrentTurnBlock::Tool { tool } = &mut block {
                    tool.raw_arguments =
                        serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
                    sync_widget_projection(tool);
                }
                current.blocks.push(block);
            }
        }
        StreamEvent::ToolCallStarted { invocation_id, tool_name, arguments, started_at_ms } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.tool_name = tool_name.clone();
                tool.arguments = normalize_object_value(arguments);
                if tool.raw_arguments.is_empty() {
                    tool.raw_arguments =
                        serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                }
                tool.started_at_ms = Some(
                    tool.started_at_ms
                        .map(|existing| existing.min(*started_at_ms))
                        .unwrap_or(*started_at_ms),
                );
                sync_widget_projection(tool);
            } else {
                let mut block = live_tool_block(
                    invocation_id.clone(),
                    tool_name.clone(),
                    normalize_object_value(arguments),
                    String::new(),
                    None,
                    *started_at_ms,
                    true,
                );
                if let CurrentTurnBlock::Tool { tool } = &mut block {
                    tool.raw_arguments =
                        serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                    sync_widget_projection(tool);
                }
                current.blocks.push(block);
            }
        }
        StreamEvent::ToolOutputDelta { invocation_id, stream, text } => {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                append_tool_output_segment(tool, stream.clone(), text);
                sync_widget_projection(tool);
            } else {
                let mut block = live_tool_block(
                    invocation_id.clone(),
                    String::new(),
                    serde_json::json!({}),
                    text.clone(),
                    Some(vec![CurrentToolOutputSegment {
                        stream: stream.clone(),
                        text: text.clone(),
                    }]),
                    now_timestamp_ms(),
                    false,
                );
                if let CurrentTurnBlock::Tool { tool } = &mut block {
                    sync_widget_projection(tool);
                }
                current.blocks.push(block);
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
                sync_widget_projection(tool);
            }
        }
        StreamEvent::WidgetHostCommand { .. }
        | StreamEvent::WidgetClientEvent { .. }
        | StreamEvent::Log { .. }
        | StreamEvent::Retrying { .. }
        | StreamEvent::Done => {}
    }
}
