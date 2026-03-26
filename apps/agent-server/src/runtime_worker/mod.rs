mod snapshots;
#[cfg(test)]
#[path = "../../tests/runtime_worker/mod.rs"]
mod tests;

use agent_runtime::TurnControl;
use axum::http::StatusCode;
use provider_registry::{ModelConfig, ProviderKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::sse::TurnStatus;

pub(crate) use snapshots::rebuild_session_snapshots_from_tape;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentToolOutput {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub detected_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub output: String,
    pub completed: bool,
    pub result_content: Option<String>,
    pub result_details: Option<serde_json::Value>,
    pub failed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurrentTurnBlock {
    Thinking { content: String },
    Tool { tool: CurrentToolOutput },
    Text { content: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentTurnSnapshot {
    pub turn_id: String,
    pub started_at_ms: u64,
    pub user_message: String,
    pub status: TurnStatus,
    pub blocks: Vec<CurrentTurnBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfoSnapshot {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

impl ProviderInfoSnapshot {
    pub fn from_identity(identity: &agent_core::ModelIdentity) -> Self {
        Self { name: identity.provider.clone(), model: identity.name.clone(), connected: true }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeWorkerError {
    pub status: StatusCode,
    pub message: String,
}

impl RuntimeWorkerError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn unavailable() -> Self {
        Self::internal("runtime worker unavailable")
    }
}

#[derive(Clone)]
pub struct CreateProviderInput {
    pub name: String,
    pub kind: ProviderKind,
    pub models: Vec<ModelConfig>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct UpdateProviderInput {
    pub kind: Option<ProviderKind>,
    pub models: Option<Vec<ModelConfig>>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct SwitchProviderInput {
    pub name: String,
}

#[derive(Clone)]
pub struct RunningTurnHandle {
    pub control: TurnControl,
}

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
        agent_runtime::TurnOutcome::WaitingForQuestion => TurnStatus::WaitingForQuestion,
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

pub(crate) fn current_turn_snapshot_from_lifecycle(
    lifecycle: &agent_runtime::TurnLifecycle,
) -> CurrentTurnSnapshot {
    CurrentTurnSnapshot {
        turn_id: lifecycle.turn_id.clone(),
        started_at_ms: lifecycle.started_at_ms,
        user_message: lifecycle.user_message.clone(),
        status: turn_lifecycle_status(lifecycle),
        blocks: lifecycle.blocks.clone().into_iter().filter_map(turn_block_to_current).collect(),
    }
}
