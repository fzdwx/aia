use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

pub struct GlobTool;

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "glob".into(),
            description: "Find files matching a glob pattern (respects .gitignore)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern (e.g. **/*.rs)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Base directory to search in"
                    }
                },
                "required": ["pattern"],
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
        let pattern = call.str_arg("pattern")?;
        let base = call
            .opt_str_arg("path")
            .map(|p| context.resolve_path(&p))
            .or_else(|| context.workspace_root.clone())
            .unwrap_or_else(|| PathBuf::from("."));

        let glob = globset::Glob::new(&pattern)
            .map_err(|e| CoreError::new(format!("invalid glob pattern: {e}")))?
            .compile_matcher();

        let walker = ignore::WalkBuilder::new(&base).build();

        let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let path = entry.path();
            let relative = path.strip_prefix(&base).unwrap_or(path);
            if !glob.is_match(relative) && !glob.is_match(path) {
                continue;
            }
            let mtime = entry.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(UNIX_EPOCH);
            entries.push((path.to_path_buf(), mtime));
        }

        entries.sort_by(|a, b| b.1.cmp(&a.1));

        let match_count = entries.len();
        let result =
            entries.iter().map(|(p, _)| p.display().to_string()).collect::<Vec<_>>().join("\n");

        Ok(ToolResult::from_call(call, result).with_details(serde_json::json!({
            "pattern": pattern,
            "matches": match_count,
        })))
    }
}
