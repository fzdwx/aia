use agent_core::{
    CoreError, Tool, ToolCall, ToolCallOutcome, ToolDefinition, ToolExecutionContext,
    ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::edit_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct EditTool;

fn build_edit_diff(old_string: &str, new_string: &str) -> String {
    let removed = old_string.lines().map(|line| format!("-{line}"));
    let added = new_string.lines().map(|line| format!("+{line}"));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditToolArgs {
    #[tool_schema(description = "Path to the file to edit")]
    file_path: String,
    #[tool_schema(description = "Exact text to find (must match uniquely)")]
    old_string: String,
    #[tool_schema(description = "Replacement text")]
    new_string: String,
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), edit_tool_description())
            .with_parameters_schema::<EditToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolCallOutcome, CoreError> {
        let args: EditToolArgs = call.parse_arguments()?;
        let path = context.resolve_path(&args.file_path);

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| CoreError::new(format!("failed to read {}: {e}", path.display())))?;

        let count = content.matches(&*args.old_string).count();
        match count {
            0 => Err(CoreError::new("old_string not found in file")),
            1 => {
                let new_content = content.replacen(&*args.old_string, &args.new_string, 1);
                tokio::fs::write(&path, &new_content).await.map_err(|e| {
                    CoreError::new(format!("failed to write {}: {e}", path.display()))
                })?;
                let old_lines = args.old_string.lines().count();
                let new_lines = args.new_string.lines().count();
                let diff = build_edit_diff(&args.old_string, &args.new_string);
                let file_path = path.display().to_string();
                Ok(ToolCallOutcome::completed(
                    ToolResult::from_call(call, format!("Edited {file_path}")).with_details(
                        serde_json::json!({
                            "file_path": file_path,
                            "added": new_lines,
                            "removed": old_lines,
                            "diff": diff,
                        }),
                    ),
                ))
            }
            n => Err(CoreError::new(format!(
                "old_string found {n} times; provide more context to make it unique"
            ))),
        }
    }
}

#[cfg(test)]
#[path = "../tests/edit/mod.rs"]
mod tests;
