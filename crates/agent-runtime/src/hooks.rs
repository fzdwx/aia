use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use agent_core::{
    CompletionRequest, ModelIdentity, PromptCacheConfig, RequestTimeoutConfig, ToolCall,
    ToolDefinition, ToolResult,
};

use crate::{RuntimeError, ToolInvocationOutcome, TurnLifecycle};

type BeforeAgentStartHandler =
    Arc<dyn Fn(&mut BeforeAgentStartEvent) -> Result<(), RuntimeError> + Send + Sync + 'static>;
type AgentStartHandler = Arc<dyn Fn(&AgentStartEvent) + Send + Sync + 'static>;
type InputHandler =
    Arc<dyn Fn(&mut InputEvent) -> Result<(), RuntimeError> + Send + Sync + 'static>;
type TurnStartHandler = Arc<dyn Fn(&TurnStartEvent) + Send + Sync + 'static>;
type BeforeProviderRequestHandler = Arc<
    dyn Fn(&mut BeforeProviderRequestEvent) -> Result<(), RuntimeError> + Send + Sync + 'static,
>;
type ToolCallHandler =
    Arc<dyn Fn(&mut ToolCallEvent) -> Result<(), RuntimeError> + Send + Sync + 'static>;
type ToolResultHandler =
    Arc<dyn Fn(&mut ToolResultEvent) -> Result<(), RuntimeError> + Send + Sync + 'static>;
type TurnEndHandler = Arc<dyn Fn(&TurnEndEvent) + Send + Sync + 'static>;

#[derive(Clone, Debug, PartialEq)]
pub struct BeforeAgentStartEvent {
    pub session_id: Option<String>,
    pub model: ModelIdentity,
    pub instructions: Option<String>,
    pub user_agent: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub disabled_tools: BTreeSet<String>,
    pub prompt_cache: Option<PromptCacheConfig>,
    pub request_timeout: Option<RequestTimeoutConfig>,
    pub max_tool_calls_per_turn: usize,
    pub context_pressure_threshold: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentStartEvent {
    pub session_id: Option<String>,
    pub model: ModelIdentity,
    pub instructions: Option<String>,
    pub visible_tools: Vec<ToolDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub session_id: Option<String>,
    pub input: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnStartEvent {
    pub session_id: Option<String>,
    pub turn_id: String,
    pub user_message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeforeProviderRequestEvent {
    pub session_id: Option<String>,
    pub turn_id: String,
    pub request_kind: String,
    pub step_index: u32,
    pub request: CompletionRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCallEvent {
    pub session_id: Option<String>,
    pub turn_id: String,
    pub call: ToolCall,
    pub override_result: Option<ToolResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolResultEvent {
    pub session_id: Option<String>,
    pub turn_id: String,
    pub call: ToolCall,
    pub outcome: ToolInvocationOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnEndEvent {
    pub session_id: Option<String>,
    pub turn: TurnLifecycle,
}

#[derive(Clone, Default)]
pub struct RuntimeHooks {
    pub(crate) before_agent_start: Vec<BeforeAgentStartHandler>,
    pub(crate) agent_start: Vec<AgentStartHandler>,
    pub(crate) input: Vec<InputHandler>,
    pub(crate) turn_start: Vec<TurnStartHandler>,
    pub(crate) before_provider_request: Vec<BeforeProviderRequestHandler>,
    pub(crate) tool_call: Vec<ToolCallHandler>,
    pub(crate) tool_result: Vec<ToolResultHandler>,
    pub(crate) turn_end: Vec<TurnEndHandler>,
}

impl RuntimeHooks {
    pub fn on_before_agent_start<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut BeforeAgentStartEvent) -> Result<(), RuntimeError> + Send + Sync + 'static,
    {
        self.before_agent_start.push(Arc::new(handler));
        self
    }

    pub fn on_agent_start<F>(mut self, handler: F) -> Self
    where
        F: Fn(&AgentStartEvent) + Send + Sync + 'static,
    {
        self.agent_start.push(Arc::new(handler));
        self
    }

    pub fn on_input<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut InputEvent) -> Result<(), RuntimeError> + Send + Sync + 'static,
    {
        self.input.push(Arc::new(handler));
        self
    }

    pub fn on_turn_start<F>(mut self, handler: F) -> Self
    where
        F: Fn(&TurnStartEvent) + Send + Sync + 'static,
    {
        self.turn_start.push(Arc::new(handler));
        self
    }

    pub fn on_before_provider_request<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut BeforeProviderRequestEvent) -> Result<(), RuntimeError> + Send + Sync + 'static,
    {
        self.before_provider_request.push(Arc::new(handler));
        self
    }

    pub fn on_tool_call<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut ToolCallEvent) -> Result<(), RuntimeError> + Send + Sync + 'static,
    {
        self.tool_call.push(Arc::new(handler));
        self
    }

    pub fn on_tool_result<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut ToolResultEvent) -> Result<(), RuntimeError> + Send + Sync + 'static,
    {
        self.tool_result.push(Arc::new(handler));
        self
    }

    pub fn on_turn_end<F>(mut self, handler: F) -> Self
    where
        F: Fn(&TurnEndEvent) + Send + Sync + 'static,
    {
        self.turn_end.push(Arc::new(handler));
        self
    }
}
