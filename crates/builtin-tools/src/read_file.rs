use std::fs;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".into(),
            description: "Read a file with line numbers".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Starting line number (0-based, default 0)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum lines to read (default 2000)"
                    }
                },
                "required": ["file_path"],
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
        let offset = call.opt_usize_arg("offset").unwrap_or(0);
        let limit = call.opt_usize_arg("limit").unwrap_or(2000);
        let path = context.resolve_path(&raw_path);

        let content = fs::read_to_string(&path)
            .map_err(|e| CoreError::new(format!("failed to read {}: {e}", path.display())))?;

        let selected: String = content
            .lines()
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult::from_call(call, selected))
    }
}
