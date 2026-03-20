use agent_core::{AbortSignal, Completion, CompletionUsage, ToolCall, ToolDefinition, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct TurnControl {
    abort: AbortSignal,
}

impl TurnControl {
    pub fn new(abort: AbortSignal) -> Self {
        Self { abort }
    }

    pub fn cancel(&self) {
        self.abort.abort();
    }

    pub fn abort_signal(&self) -> AbortSignal {
        self.abort.clone()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextStats {
    pub total_entries: usize,
    pub anchor_count: usize,
    pub entries_since_last_anchor: usize,
    pub last_input_tokens: Option<u32>,
    pub context_limit: Option<u32>,
    pub output_limit: Option<u32>,
    pub pressure_ratio: Option<f64>,
}

pub type RuntimeSubscriberId = u64;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolTraceContext {
    pub session_id: Option<String>,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub root_span_id: String,
    pub operation_name: String,
    pub parent_request_kind: String,
    pub parent_step_index: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocationLifecycle {
    pub call: ToolCall,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
    #[serde(default)]
    pub trace_context: Option<ToolTraceContext>,
    pub outcome: ToolInvocationOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Succeeded,
    Failed,
    Cancelled,
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
    #[serde(default)]
    pub usage: Option<CompletionUsage>,
    pub failure_message: Option<String>,
    #[serde(default = "default_turn_outcome")]
    pub outcome: TurnOutcome,
}

fn default_turn_outcome() -> TurnOutcome {
    TurnOutcome::Succeeded
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
    ToolInvocation { invocation: Box<ToolInvocationLifecycle> },
    Failure { message: String },
    Cancelled { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvent {
    UserMessage { content: String },
    AssistantMessage { content: String },
    ToolInvocation { call: ToolCall, outcome: ToolInvocationOutcome },
    TurnLifecycle { turn: TurnLifecycle },
    TurnFailed { message: String },
    ContextCompressed { summary: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnOutput {
    pub assistant_text: String,
    pub completion: Completion,
    pub visible_tools: Vec<ToolDefinition>,
}
