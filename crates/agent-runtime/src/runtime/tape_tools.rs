use std::{cell::RefCell, rc::Rc};

use agent_core::{
    CoreError, LanguageModel, RuntimeToolContext, RuntimeToolContextStats, Tool, ToolCall,
    ToolDefinition, ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolRegistry, ToolResult,
};
use serde_json::json;

use super::AgentRuntime;

pub(super) fn build_runtime_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(TapeInfoTool));
    registry.register(Box::new(TapeHandoffTool));
    registry
}

pub(super) struct RuntimeToolContextBridge {
    runtime: RefCell<*mut ()>,
    context_stats_fn: fn(*mut ()) -> RuntimeToolContextStats,
    record_handoff_fn: fn(*mut (), &str, &str) -> Result<(), CoreError>,
}

impl RuntimeToolContextBridge {
    pub(super) fn new<M, T>(runtime: &mut AgentRuntime<M, T>) -> Rc<Self>
    where
        M: LanguageModel,
        T: ToolExecutor,
    {
        fn context_stats_impl<M, T>(runtime: *mut ()) -> RuntimeToolContextStats
        where
            M: LanguageModel,
            T: ToolExecutor,
        {
            let runtime = unsafe { &mut *runtime.cast::<AgentRuntime<M, T>>() };
            let stats = runtime.context_stats();
            RuntimeToolContextStats {
                total_entries: stats.total_entries,
                anchor_count: stats.anchor_count,
                entries_since_last_anchor: stats.entries_since_last_anchor,
                estimated_context_units: stats.estimated_context_units,
                context_limit: stats.context_limit,
                output_limit: stats.output_limit,
                pressure_ratio: stats.pressure_ratio,
            }
        }

        fn record_handoff_impl<M, T>(
            runtime: *mut (),
            name: &str,
            summary: &str,
        ) -> Result<(), CoreError>
        where
            M: LanguageModel,
            T: ToolExecutor,
        {
            let runtime = unsafe { &mut *runtime.cast::<AgentRuntime<M, T>>() };
            runtime
                .record_handoff(name, json!({ "summary": summary }), "ai")
                .map(|_| ())
                .map_err(|error| CoreError::new(error.to_string()))
        }

        Rc::new(Self {
            runtime: RefCell::new(runtime as *mut AgentRuntime<M, T> as *mut ()),
            context_stats_fn: context_stats_impl::<M, T>,
            record_handoff_fn: record_handoff_impl::<M, T>,
        })
    }
}

impl RuntimeToolContext for RuntimeToolContextBridge {
    fn context_stats(&self) -> RuntimeToolContextStats {
        (self.context_stats_fn)(*self.runtime.borrow())
    }

    fn record_handoff(&self, name: &str, summary: &str) -> Result<(), CoreError> {
        (self.record_handoff_fn)(*self.runtime.borrow(), name, summary)
    }
}

struct TapeInfoTool;

impl Tool for TapeInfoTool {
    fn name(&self) -> &str {
        "tape.info"
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
             estimated_context_units: {}\n\
             context_limit: {}\n\
             pressure_ratio: {:.2}",
            stats.total_entries,
            stats.anchor_count,
            stats.entries_since_last_anchor,
            stats.estimated_context_units,
            stats.context_limit.map_or("unknown".to_string(), |value| value.to_string()),
            stats.pressure_ratio.unwrap_or(0.0),
        );
        Ok(ToolResult::from_call(tool_call, content))
    }
}

struct TapeHandoffTool;

impl Tool for TapeHandoffTool {
    fn name(&self) -> &str {
        "tape.handoff"
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
