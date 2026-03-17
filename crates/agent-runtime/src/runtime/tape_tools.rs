use std::sync::{Arc, Mutex};

use agent_core::{
    CoreError, LanguageModel, RuntimeToolContext, RuntimeToolContextStats, Tool, ToolCall,
    ToolDefinition, ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolRegistry, ToolResult,
};
use agent_prompts::tool_descriptions::{tape_handoff_tool_description, tape_info_tool_description};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

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
    pending_handoffs: Mutex<Vec<(String, String)>>,
}

impl RuntimeToolContextBridge {
    pub(super) fn new<M, T>(runtime: &AgentRuntime<M, T>) -> Arc<Self>
    where
        M: LanguageModel,
        T: ToolExecutor,
    {
        let stats = runtime.context_stats();
        Arc::new(Self {
            total_entries: stats.total_entries,
            anchor_count: stats.anchor_count,
            entries_since_last_anchor: stats.entries_since_last_anchor,
            last_input_tokens: stats.last_input_tokens,
            context_limit: stats.context_limit,
            output_limit: stats.output_limit,
            pressure_ratio: stats.pressure_ratio,
            pending_handoffs: Mutex::new(Vec::new()),
        })
    }

    pub(super) fn drain_handoffs(&self) -> Vec<(String, String)> {
        let mut guard =
            self.pending_handoffs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *guard)
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
        self.pending_handoffs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((name.to_string(), summary.to_string()));
        Ok(())
    }
}

struct TapeInfoTool;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TapeInfoToolArgs {}

#[async_trait]
impl Tool for TapeInfoTool {
    fn name(&self) -> &str {
        "tape_info"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), tape_info_tool_description())
            .with_parameters_schema::<TapeInfoToolArgs>()
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let _: TapeInfoToolArgs = tool_call.parse_arguments()?;
        let runtime = context
            .runtime
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool context unavailable"))?;
        let stats = runtime.context_stats();
        let details = json!({
            "entries": stats.total_entries,
            "anchors": stats.anchor_count,
            "entries_since_last_anchor": stats.entries_since_last_anchor,
            "last_input_tokens": stats.last_input_tokens,
            "context_limit": stats.context_limit,
            "output_limit": stats.output_limit,
            "pressure_ratio": stats.pressure_ratio,
        });
        let content = serde_json::to_string_pretty(&details)
            .map_err(|error| CoreError::new(format!("failed to serialize tape_info: {error}")))?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}

struct TapeHandoffTool;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TapeHandoffToolArgs {
    #[schemars(description = "A concise summary of the conversation so far to carry forward.")]
    summary: String,
    #[schemars(description = "Optional name for the anchor (default: \"handoff\").")]
    name: Option<String>,
}

#[async_trait]
impl Tool for TapeHandoffTool {
    fn name(&self) -> &str {
        "tape_handoff"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), tape_handoff_tool_description())
            .with_parameters_schema::<TapeHandoffToolArgs>()
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: TapeHandoffToolArgs = tool_call.parse_arguments()?;
        let runtime = context
            .runtime
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool context unavailable"))?;
        let name = args.name.as_deref().unwrap_or("handoff");

        runtime.record_handoff(name, &args.summary)?;
        Ok(ToolResult::from_call(tool_call, format!("anchor added: {name}")))
    }
}

pub(super) fn runtime_tool_definitions() -> Vec<ToolDefinition> {
    build_runtime_tool_registry().definitions()
}

pub(super) fn is_runtime_tool(name: &str) -> bool {
    build_runtime_tool_registry().contains(name)
}

#[cfg(test)]
mod tests {
    use agent_core::ToolDefinition;
    use agent_prompts::tool_descriptions::{
        tape_handoff_tool_description, tape_info_tool_description,
    };

    use super::{
        TapeHandoffTool, TapeHandoffToolArgs, TapeInfoTool, TapeInfoToolArgs,
        runtime_tool_definitions,
    };
    use crate::runtime::tape_tools::Tool;

    #[test]
    fn runtime_tool_definitions_match_schemars_output() {
        let definitions = runtime_tool_definitions();
        assert_eq!(definitions.len(), 2);

        let tape_info = TapeInfoTool.definition();
        assert!(definitions.iter().any(|definition| definition == &tape_info));
        assert_eq!(
            tape_info.parameters,
            ToolDefinition::new("tape_info", tape_info_tool_description())
                .with_parameters_schema::<TapeInfoToolArgs>()
                .parameters
        );

        let tape_handoff = TapeHandoffTool.definition();
        assert!(definitions.iter().any(|definition| definition == &tape_handoff));
        assert_eq!(
            tape_handoff.parameters,
            ToolDefinition::new("tape_handoff", tape_handoff_tool_description())
                .with_parameters_schema::<TapeHandoffToolArgs>()
                .parameters
        );
    }
}
