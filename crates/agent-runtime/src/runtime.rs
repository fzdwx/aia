mod compress;
mod error;
mod events;
mod finalize;
mod helpers;
mod request;
mod tape_tools;
#[cfg(test)]
mod tests;
mod tool_calls;
mod turn;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agent_core::{LanguageModel, ModelIdentity, ToolDefinition, ToolExecutor};
use session_tape::{Handoff, SessionTape, SessionTapeError, TapeEntry};

use crate::{RuntimeEvent, RuntimeSubscriberId};

pub use self::error::RuntimeError;

const DEFAULT_MAX_TOOL_CALLS_PER_TURN: usize = 1000;
const DEFAULT_CONTEXT_PRESSURE_THRESHOLD: f64 = agent_prompts::AUTO_COMPRESSION_THRESHOLD;
type TapeEntryListener =
    Arc<dyn Fn(&TapeEntry) -> Result<(), SessionTapeError> + Send + Sync + 'static>;

pub struct AgentRuntime<M, T> {
    model: M,
    tools: T,
    tape: SessionTape,
    model_identity: ModelIdentity,
    instructions: Option<String>,
    disabled_tools: BTreeSet<String>,
    workspace_root: Option<std::path::PathBuf>,
    max_tool_calls_per_turn: usize,
    context_pressure_threshold: f64,
    tape_entry_listener: Option<TapeEntryListener>,
    events: Vec<RuntimeEvent>,
    subscribers: BTreeMap<RuntimeSubscriberId, usize>,
    next_subscriber_id: RuntimeSubscriberId,
}

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
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
            max_tool_calls_per_turn: DEFAULT_MAX_TOOL_CALLS_PER_TURN,
            context_pressure_threshold: DEFAULT_CONTEXT_PRESSURE_THRESHOLD,
            tape_entry_listener: None,
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

    pub fn with_max_tool_calls_per_turn(mut self, max_tool_calls_per_turn: usize) -> Self {
        self.max_tool_calls_per_turn = max_tool_calls_per_turn.max(1);
        self
    }

    pub fn with_context_pressure_threshold(mut self, threshold: f64) -> Self {
        self.context_pressure_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub fn with_tape_entry_listener(
        mut self,
        listener: impl Fn(&TapeEntry) -> Result<(), SessionTapeError> + Send + Sync + 'static,
    ) -> Self {
        self.tape_entry_listener = Some(Arc::new(listener));
        self
    }

    pub fn disable_tool(&mut self, tool_name: impl Into<String>) {
        self.disabled_tools.insert(tool_name.into());
    }

    pub fn enable_tool(&mut self, tool_name: &str) {
        self.disabled_tools.remove(tool_name);
    }

    pub fn visible_tools(&self) -> Vec<ToolDefinition> {
        let mut tools: Vec<ToolDefinition> = self
            .tools
            .definitions()
            .into_iter()
            .filter(|definition| !self.disabled_tools.contains(&definition.name))
            .collect();
        tools.extend(tape_tools::runtime_tool_definitions());
        tools
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

    pub(super) fn append_tape_entry(&mut self, entry: TapeEntry) -> Result<u64, RuntimeError> {
        let entry_id = self.tape.append_entry(entry);
        self.persist_last_tape_entry()?;
        Ok(entry_id)
    }

    pub(super) fn record_handoff(
        &mut self,
        name: impl Into<String>,
        state: serde_json::Value,
        source: &str,
    ) -> Result<Handoff, RuntimeError> {
        let previous_len = self.tape.entries().len();
        let handoff = self.tape.handoff(name, state);
        self.tape.set_entry_meta(
            handoff.anchor.entry_id,
            "source",
            serde_json::Value::String(source.to_string()),
        );
        self.persist_tape_entries_from(previous_len)?;
        Ok(handoff)
    }

    fn persist_last_tape_entry(&self) -> Result<(), RuntimeError> {
        let Some(listener) = self.tape_entry_listener.as_ref() else {
            return Ok(());
        };
        let Some(entry) = self.tape.entries().last() else {
            return Err(RuntimeError::session("会话条目追加后未找到最后一条记录"));
        };
        listener(entry).map_err(RuntimeError::session)
    }

    fn persist_tape_entries_from(&self, start_index: usize) -> Result<(), RuntimeError> {
        let Some(listener) = self.tape_entry_listener.as_ref() else {
            return Ok(());
        };
        for entry in &self.tape.entries()[start_index..] {
            listener(entry).map_err(RuntimeError::session)?;
        }
        Ok(())
    }
}
