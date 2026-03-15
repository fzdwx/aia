use std::{cell::RefCell, rc::Rc};

use agent_core::{
    CoreError, LanguageModel, RuntimeToolContext, RuntimeToolContextStats, Tool, ToolCall,
    ToolDefinition, ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolRegistry, ToolResult,
};

use super::AgentRuntime;

pub(super) fn build_runtime_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(TapeInfoTool));
    registry.register(Box::new(TapeHandoffTool));
    registry
}

pub(super) struct RuntimeToolContextBridge {
    total_entries: usize,
    anchor_count: usize,
    entries_since_last_anchor: usize,
    last_input_tokens: Option<u32>,
    context_limit: Option<u32>,
    output_limit: Option<u32>,
    pressure_ratio: Option<f64>,
    pending_handoffs: RefCell<Vec<(String, String)>>,
}

impl RuntimeToolContextBridge {
    pub(super) fn new<M, T>(runtime: &AgentRuntime<M, T>) -> Rc<Self>
    where
        M: LanguageModel,
        T: ToolExecutor,
    {
        let stats = runtime.context_stats();
        Rc::new(Self {
            total_entries: stats.total_entries,
            anchor_count: stats.anchor_count,
            entries_since_last_anchor: stats.entries_since_last_anchor,
            last_input_tokens: stats.last_input_tokens,
            context_limit: stats.context_limit,
            output_limit: stats.output_limit,
            pressure_ratio: stats.pressure_ratio,
            pending_handoffs: RefCell::new(Vec::new()),
        })
    }

    pub(super) fn drain_handoffs(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.pending_handoffs.borrow_mut())
    }
}

impl RuntimeToolContext for RuntimeToolContextBridge {
    fn context_stats(&self) -> RuntimeToolContextStats {
        RuntimeToolContextStats {
            total_entries: self.total_entries,
            anchor_count: self.anchor_count,
            entries_since_last_anchor: self.entries_since_last_anchor,
            last_input_tokens: self.last_input_tokens,
            context_limit: self.context_limit,
            output_limit: self.output_limit,
            pressure_ratio: self.pressure_ratio,
        }
    }

    fn record_handoff(&self, name: &str, summary: &str) -> Result<(), CoreError> {
        self.pending_handoffs.borrow_mut().push((name.to_string(), summary.to_string()));
        Ok(())
    }
}

struct TapeInfoTool;

impl Tool for TapeInfoTool {
    fn name(&self) -> &str {
        "tape_info"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), "Return context usage statistics for the current session.")
    }

    fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let runtime = context
            .runtime
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool context unavailable"))?;
        let stats = runtime.context_stats();
        let content = format!(
            "entries: {}\n\
             anchors: {}\n\
             entries_since_last_anchor: {}\n\
             last_input_tokens: {}\n\
             context_limit: {}\n\
             pressure_ratio: {:.2}",
            stats.total_entries,
            stats.anchor_count,
            stats.entries_since_last_anchor,
            stats.last_input_tokens.map_or("unknown".to_string(), |value| value.to_string()),
            stats.context_limit.map_or("unknown".to_string(), |value| value.to_string()),
            stats.pressure_ratio.unwrap_or(0.0),
        );
        Ok(ToolResult::from_call(tool_call, content))
    }
}

struct TapeHandoffTool;

impl Tool for TapeHandoffTool {
    fn name(&self) -> &str {
        "tape_handoff"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            self.name(),
            "Create an anchor to truncate history and carry forward a summary as minimal inherited state.",
        )
        .with_parameter(
            "summary",
            "A concise summary of the conversation so far to carry forward.",
            true,
        )
        .with_parameter("name", "Optional name for the anchor (default: \"handoff\").", false)
    }

    fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let runtime = context
            .runtime
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool context unavailable"))?;
        let summary =
            tool_call.arguments.get("summary").and_then(|value| value.as_str()).unwrap_or("");
        let name =
            tool_call.arguments.get("name").and_then(|value| value.as_str()).unwrap_or("handoff");

        runtime.record_handoff(name, summary)?;
        Ok(ToolResult::from_call(tool_call, format!("anchor added: {name}")))
    }
}

pub(super) fn runtime_tool_definitions() -> Vec<ToolDefinition> {
    build_runtime_tool_registry().definitions()
}

pub(super) fn is_runtime_tool(name: &str) -> bool {
    build_runtime_tool_registry().contains(name)
}
