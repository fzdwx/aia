use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::write_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct WriteTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteToolArgs {
    #[tool_schema(description = "Path to write to")]
    file_path: String,
    #[tool_schema(description = "Content to write")]
    content: String,
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), write_tool_description())
            .with_parameters_schema::<WriteToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: WriteToolArgs = call.parse_arguments()?;
        let path = context.resolve_path(&args.file_path);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| CoreError::new(format!("failed to create directory: {e}")))?;
        }

        let bytes = args.content.len();
        let lines = args.content.lines().count();
        tokio::fs::write(&path, &args.content)
            .await
            .map_err(|e| CoreError::new(format!("failed to write {}: {e}", path.display())))?;

        Ok(ToolResult::from_call(call, format!("Wrote {bytes} bytes to {}", path.display()))
            .with_details(serde_json::json!({
                "file_path": path.display().to_string(),
                "lines": lines,
            })))
    }
}

#[cfg(test)]
#[path = "../tests/write/mod.rs"]
mod tests;
