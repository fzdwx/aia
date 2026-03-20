use std::io::ErrorKind;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::read_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct ReadTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadToolArgs {
    #[tool_schema(description = "Path to the file to read")]
    file_path: String,
    #[tool_schema(description = "Starting line number (0-based, default 0)")]
    offset: Option<usize>,
    #[tool_schema(description = "Maximum lines to read (default 2000)")]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), read_tool_description())
            .with_parameters_schema::<ReadToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: ReadToolArgs = call.parse_arguments()?;
        let offset = args.offset.unwrap_or(0);
        let limit = args.limit.unwrap_or(2000);
        let path = context.resolve_path(&args.file_path);

        let content =
            tokio::fs::read_to_string(&path).await.map_err(|error| match error.kind() {
                ErrorKind::InvalidData => CoreError::new(format!(
                    "failed to read {}: file is not valid UTF-8 text",
                    path.display()
                )),
                _ => CoreError::new(format!("failed to read {}: {error}", path.display())),
            })?;

        let total_lines = content.lines().count();

        let selected: String = content
            .lines()
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        let lines_read = selected.lines().count();

        Ok(ToolResult::from_call(call, selected).with_details(serde_json::json!({
            "file_path": path.display().to_string(),
            "lines_read": lines_read,
            "total_lines": total_lines,
        })))
    }
}

#[cfg(test)]
#[path = "../tests/read/mod.rs"]
mod tests;
