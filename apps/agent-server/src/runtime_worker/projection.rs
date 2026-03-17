use serde_json::{Value, json};

use crate::sse::TurnStatus;

use super::{CurrentToolOutput, CurrentTurnBlock};

pub(crate) fn normalize_object_value(value: &Value) -> Value {
    if value.is_object() { value.clone() } else { json!({}) }
}

pub(crate) fn find_tool_output_mut<'a>(
    blocks: &'a mut [CurrentTurnBlock],
    invocation_id: &str,
) -> Option<&'a mut CurrentToolOutput> {
    blocks.iter_mut().rev().find_map(|block| match block {
        CurrentTurnBlock::Tool { tool } if tool.invocation_id == invocation_id => Some(tool),
        _ => None,
    })
}

pub(crate) fn live_tool_block(
    invocation_id: String,
    tool_name: String,
    arguments: Value,
    output: String,
    timestamp_ms: u64,
    started: bool,
) -> CurrentTurnBlock {
    CurrentTurnBlock::Tool {
        tool: CurrentToolOutput {
            invocation_id,
            tool_name,
            arguments,
            detected_at_ms: timestamp_ms,
            started_at_ms: started.then_some(timestamp_ms),
            finished_at_ms: None,
            output,
            completed: false,
            result_content: None,
            result_details: None,
            failed: None,
        },
    }
}

pub(crate) fn turn_lifecycle_status(lifecycle: &agent_runtime::TurnLifecycle) -> TurnStatus {
    match lifecycle.outcome {
        agent_runtime::TurnOutcome::Cancelled => TurnStatus::Cancelled,
        _ if lifecycle
            .blocks
            .iter()
            .any(|block| matches!(block, agent_runtime::TurnBlock::ToolInvocation { .. })) =>
        {
            TurnStatus::Working
        }
        _ if lifecycle
            .blocks
            .iter()
            .any(|block| matches!(block, agent_runtime::TurnBlock::Assistant { .. })) =>
        {
            TurnStatus::Generating
        }
        _ if lifecycle
            .blocks
            .iter()
            .any(|block| matches!(block, agent_runtime::TurnBlock::Thinking { .. })) =>
        {
            TurnStatus::Thinking
        }
        _ => TurnStatus::Waiting,
    }
}

pub(crate) fn turn_block_to_current(block: agent_runtime::TurnBlock) -> Option<CurrentTurnBlock> {
    match block {
        agent_runtime::TurnBlock::Thinking { content } => {
            Some(CurrentTurnBlock::Thinking { content })
        }
        agent_runtime::TurnBlock::Assistant { content } => Some(CurrentTurnBlock::Text { content }),
        agent_runtime::TurnBlock::ToolInvocation { invocation } => {
            let invocation = *invocation;
            let (result_content, result_details, failed) = match invocation.outcome {
                agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                    (Some(result.content), result.details, Some(false))
                }
                agent_runtime::ToolInvocationOutcome::Failed { message } => {
                    (Some(message), None, Some(true))
                }
            };

            Some(CurrentTurnBlock::Tool {
                tool: CurrentToolOutput {
                    invocation_id: invocation.call.invocation_id,
                    tool_name: invocation.call.tool_name,
                    arguments: normalize_object_value(&invocation.call.arguments),
                    detected_at_ms: invocation.started_at_ms,
                    started_at_ms: Some(invocation.started_at_ms),
                    finished_at_ms: Some(invocation.finished_at_ms),
                    output: String::new(),
                    completed: true,
                    result_content,
                    result_details,
                    failed,
                },
            })
        }
        agent_runtime::TurnBlock::Failure { .. } | agent_runtime::TurnBlock::Cancelled { .. } => {
            None
        }
    }
}
