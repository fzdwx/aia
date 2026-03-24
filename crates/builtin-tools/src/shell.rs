mod capture;
mod execution;
#[cfg(test)]
#[path = "../tests/shell/mod.rs"]
mod tests;

use std::path::Path;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::shell_tool_description;
use async_trait::async_trait;
use execution::run_embedded_brush;
use serde::{Deserialize, Serialize};

pub struct ShellTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShellToolArgs {
    #[tool_schema(description = "The shell command to execute")]
    command: String,
    #[tool_schema(
        description = "Clear, concise description of what this command does in 5-10 words. Examples:\nInput: ls\nOutput: Lists files in current directory\n\nInput: git status\nOutput: Shows working tree status\n\nInput: npm install\nOutput: Installs package dependencies\n\nInput: mkdir foo\nOutput: Creates directory 'foo'"
    )]
    description: Option<String>,
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "Shell"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), shell_tool_description())
            .with_parameters_schema::<ShellToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: ShellToolArgs = call.parse_arguments()?;
        let command = args.command;
        let _description = args.description;
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
