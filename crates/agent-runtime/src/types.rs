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
    WaitingForQuestion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnLifecycle {
    pub turn_id: String,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
    pub source_entry_ids: Vec<u64>,
    /// 用户消息列表，多条消息时有多个元素
    pub user_messages: Vec<String>,
    pub blocks: Vec<TurnBlock>,
    pub assistant_message: Option<String>,
    pub thinking: Option<String>,
    pub tool_invocations: Vec<ToolInvocationLifecycle>,
    pub usage: Option<CompletionUsage>,
    pub failure_message: Option<String>,
    pub outcome: TurnOutcome,
}

impl<'de> serde::Deserialize<'de> for TurnLifecycle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct LegacyTurnLifecycle {
            turn_id: String,
            started_at_ms: u64,
            finished_at_ms: u64,
            source_entry_ids: Vec<u64>,
            /// 旧字段（向后兼容）
            user_message: Option<String>,
            /// 新字段
            user_messages: Option<Vec<String>>,
            blocks: Vec<TurnBlock>,
            assistant_message: Option<String>,
            thinking: Option<String>,
            tool_invocations: Vec<ToolInvocationLifecycle>,
            #[serde(default)]
            usage: Option<CompletionUsage>,
            failure_message: Option<String>,
            #[serde(default = "default_turn_outcome")]
            outcome: TurnOutcome,
        }

        let legacy = LegacyTurnLifecycle::deserialize(deserializer)?;
        
        // 向后兼容：如果 user_messages 为空但有 user_message，把 user_message 放进去
        let user_messages = if let Some(messages) = legacy.user_messages {
            messages
        } else if let Some(message) = legacy.user_message {
            vec![message]
        } else {
            Vec::new()
        };

        Ok(TurnLifecycle {
            turn_id: legacy.turn_id,
            started_at_ms: legacy.started_at_ms,
            finished_at_ms: legacy.finished_at_ms,
            source_entry_ids: legacy.source_entry_ids,
            user_messages,
            blocks: legacy.blocks,
            assistant_message: legacy.assistant_message,
            thinking: legacy.thinking,
            tool_invocations: legacy.tool_invocations,
            usage: legacy.usage,
            failure_message: legacy.failure_message,
            outcome: legacy.outcome,
        })
    }
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
