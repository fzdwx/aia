use std::fs;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

pub struct WriteFileTool;

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".into(),
            description: "Create or overwrite a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to write to"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    }
                },
                "required": ["file_path", "content"],
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
        let content = call.str_arg("content")?;
        let path = context.resolve_path(&raw_path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| CoreError::new(format!("failed to create directory: {e}")))?;
        }

        let bytes = content.len();
        fs::write(&path, &content)
            .map_err(|e| CoreError::new(format!("failed to write {}: {e}", path.display())))?;

        Ok(ToolResult::from_call(call, format!("Wrote {bytes} bytes to {}", path.display())))
    }
}
