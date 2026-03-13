use std::fs;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

pub struct EditTool;

impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit".into(),
            description: "Replace exact text in a file (must match uniquely)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact text to find (must appear exactly once)"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Replacement text"
                    }
                },
                "required": ["file_path", "old_string", "new_string"],
                "additionalProperties": false
            }),
        }
    }

    fn call(
        &self,
        call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let raw_path = call.str_arg("file_path")?;
        let old_string = call.str_arg("old_string")?;
        let new_string = call.str_arg("new_string")?;
        let path = context.resolve_path(&raw_path);

        let content = fs::read_to_string(&path)
            .map_err(|e| CoreError::new(format!("failed to read {}: {e}", path.display())))?;

        let count = content.matches(&*old_string).count();
        match count {
            0 => Err(CoreError::new("old_string not found in file")),
            1 => {
                let new_content = content.replacen(&*old_string, &new_string, 1);
                fs::write(&path, &new_content).map_err(|e| {
                    CoreError::new(format!("failed to write {}: {e}", path.display()))
                })?;
                let old_lines = old_string.lines().count();
                let new_lines = new_string.lines().count();
                Ok(ToolResult::from_call(call, format!("Edited {}", path.display())).with_details(
                    serde_json::json!({
                        "file_path": path.display().to_string(),
                        "added": new_lines,
                        "removed": old_lines,
                    }),
                ))
            }
            n => Err(CoreError::new(format!(
                "old_string found {n} times; provide more context to make it unique"
            ))),
        }
    }
}
