use agent_core::{Completion, ToolCall, ToolDefinition, ToolResult};
use serde::{Deserialize, Serialize};

pub type RuntimeSubscriberId = u64;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocationLifecycle {
    pub call: ToolCall,
    pub outcome: ToolInvocationOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnLifecycle {
    pub turn_id: String,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
    pub source_entry_ids: Vec<u64>,
    pub user_message: String,
    pub blocks: Vec<TurnBlock>,
    pub assistant_message: Option<String>,
    pub thinking: Option<String>,
    pub tool_invocations: Vec<ToolInvocationLifecycle>,
    pub failure_message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolInvocationOutcome {
    Succeeded { result: ToolResult },
    Failed { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnBlock {
    Thinking { content: String },
    Assistant { content: String },
    ToolInvocation { invocation: ToolInvocationLifecycle },
    Failure { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvent {
    UserMessage { content: String },
    AssistantMessage { content: String },
    ToolInvocation { call: ToolCall, outcome: ToolInvocationOutcome },
    TurnLifecycle { turn: TurnLifecycle },
    TurnFailed { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnOutput {
    pub assistant_text: String,
    pub completion: Completion,
    pub visible_tools: Vec<ToolDefinition>,
}
