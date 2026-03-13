mod error;
mod events;
mod finalize;
mod helpers;
mod request;
#[cfg(test)]
mod tests;
mod tool_calls;
mod turn;

use std::collections::{BTreeMap, BTreeSet};

use agent_core::{LanguageModel, ModelIdentity, ToolDefinition, ToolExecutor};
use session_tape::{Handoff, SessionTape};

use crate::{RuntimeEvent, RuntimeSubscriberId};

pub use self::error::RuntimeError;

const DEFAULT_MAX_TURN_STEPS: usize = 8;
const DEFAULT_MAX_TOOL_CALLS_PER_TURN: usize = 50;

pub struct AgentRuntime<M, T> {
    model: M,
    tools: T,
    tape: SessionTape,
    model_identity: ModelIdentity,
    instructions: Option<String>,
    disabled_tools: BTreeSet<String>,
    workspace_root: Option<std::path::PathBuf>,
    max_turn_steps: usize,
    max_tool_calls_per_turn: usize,
    events: Vec<RuntimeEvent>,
    subscribers: BTreeMap<RuntimeSubscriberId, usize>,
    next_subscriber_id: RuntimeSubscriberId,
}

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    const FINAL_TEXT_ONLY_INSTRUCTION: &'static str = "你已经到达本轮内部步骤预算的最后一步。不要再调用任何工具，直接基于当前上下文给出最好的最终回答。";
    const STEP_BUDGET_INSTRUCTION_PREFIX: &'static str = "当前是本轮内部循环的预算提示：";

    pub fn new(model: M, tools: T, model_identity: ModelIdentity) -> Self {
        Self::with_tape(model, tools, model_identity, SessionTape::new())
    }

    pub fn with_tape(model: M, tools: T, model_identity: ModelIdentity, tape: SessionTape) -> Self {
        Self {
            model,
            tools,
            tape,
            model_identity,
            instructions: None,
            disabled_tools: BTreeSet::new(),
            workspace_root: None,
            max_turn_steps: DEFAULT_MAX_TURN_STEPS,
            max_tool_calls_per_turn: DEFAULT_MAX_TOOL_CALLS_PER_TURN,
            events: Vec::new(),
            subscribers: BTreeMap::new(),
            next_subscriber_id: 1,
        }
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    pub fn with_workspace_root(mut self, workspace_root: impl Into<std::path::PathBuf>) -> Self {
        self.workspace_root = Some(workspace_root.into());
        self
    }

    pub fn with_max_turn_steps(mut self, max_turn_steps: usize) -> Self {
        self.max_turn_steps = max_turn_steps.max(1);
        self
    }

    pub fn with_max_tool_calls_per_turn(mut self, max_tool_calls_per_turn: usize) -> Self {
        self.max_tool_calls_per_turn = max_tool_calls_per_turn.max(1);
        self
    }

    pub fn disable_tool(&mut self, tool_name: impl Into<String>) {
        self.disabled_tools.insert(tool_name.into());
    }

    pub fn enable_tool(&mut self, tool_name: &str) {
        self.disabled_tools.remove(tool_name);
    }

    pub fn visible_tools(&self) -> Vec<ToolDefinition> {
        self.tools
            .definitions()
            .into_iter()
            .filter(|definition| !self.disabled_tools.contains(&definition.name))
            .collect()
    }

    pub fn handoff(&mut self, name: impl Into<String>, state: serde_json::Value) -> Handoff {
        self.tape.handoff(name, state)
    }

    pub fn tape(&self) -> &SessionTape {
        &self.tape
    }

    pub fn tape_mut(&mut self) -> &mut SessionTape {
        &mut self.tape
    }

    pub fn model_identity(&self) -> &ModelIdentity {
        &self.model_identity
    }

    pub fn replace_model(&mut self, model: M, identity: ModelIdentity) {
        self.model = model;
        self.model_identity = identity;
    }
}
