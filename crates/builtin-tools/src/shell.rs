mod capture;
mod execution;
#[cfg(test)]
mod tests;

use std::path::Path;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_prompts::tool_descriptions::shell_tool_description;
use async_trait::async_trait;
use execution::run_embedded_brush;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct ShellTool;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShellToolArgs {
    command: String,
}

pub(crate) fn shell_tool_parameters() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "command": {
                "description": "The shell command to execute",
                "type": "string"
            }
        },
        "required": ["command"],
        "additionalProperties": false
    })
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), shell_tool_description())
            .with_parameters_value(shell_tool_parameters())
    }

    async fn call(
        &self,
        call: &ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: ShellToolArgs = call.parse_arguments()?;
        let command = args.command;
        let cwd = context.workspace_root.as_deref().unwrap_or_else(|| Path::new("."));

        if context.abort.is_aborted() {
            return Ok(ToolResult::from_call(call, "[aborted]"));
        }

        let execution = run_embedded_brush(&command, cwd, &context.abort, output).await?;

        let mut result_text = execution.stdout.clone();
        if !execution.stderr.is_empty() {
            result_text.push_str(&execution.stderr);
        }
        if execution.exit_code != 0 {
            result_text.push_str(&format!("\n[exit code: {}]", execution.exit_code));
        }

        Ok(ToolResult::from_call(call, result_text).with_details(serde_json::json!({
            "command": command,
            "exit_code": execution.exit_code,
            "stdout": execution.stdout,
            "stderr": execution.stderr,
        })))
    }
}
