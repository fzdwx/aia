use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::{tape_handoff_tool_description, tape_info_tool_description};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct TapeInfoTool;
pub struct TapeHandoffTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TapeInfoToolArgs {}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TapeHandoffToolArgs {
    #[tool_schema(description = "A concise summary of the conversation so far to carry forward.")]
    summary: String,
    #[tool_schema(description = "Optional name for the anchor (default: \"handoff\").")]
    name: Option<String>,
}

#[async_trait]
impl Tool for TapeInfoTool {
    fn name(&self) -> &str {
        "TapeInfo"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), tape_info_tool_description())
            .with_parameters_schema::<TapeInfoToolArgs>()
    }

    fn requires_runtime_context(&self) -> bool {
        true
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
            .map_err(|error| CoreError::new(format!("failed to serialize TapeInfo: {error}")))?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}

#[async_trait]
impl Tool for TapeHandoffTool {
    fn name(&self) -> &str {
        "TapeHandoff"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), tape_handoff_tool_description())
            .with_parameters_schema::<TapeHandoffToolArgs>()
    }

    fn requires_runtime_context(&self) -> bool {
        true
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
