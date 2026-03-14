use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

use crate::should_skip_directory;

pub struct GlobTool;
const DEFAULT_MATCH_LIMIT: usize = 200;
const MAX_MATCH_LIMIT: usize = 1000;

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "glob".into(),
            description:
                "Find files matching a glob pattern (respects .gitignore and skips .git/node_modules/target)"
                    .into(),
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
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum matched files to return (default 200, max 1000)"
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
        let limit = call.opt_usize_arg("limit").unwrap_or(DEFAULT_MATCH_LIMIT).min(MAX_MATCH_LIMIT);
        let base = call
            .opt_str_arg("path")
            .map(|p| context.resolve_path(&p))
            .or_else(|| context.workspace_root.clone())
            .unwrap_or_else(|| PathBuf::from("."));

        let glob = globset::Glob::new(&pattern)
            .map_err(|e| CoreError::new(format!("invalid glob pattern: {e}")))?
            .compile_matcher();

        let mut builder = ignore::WalkBuilder::new(&base);
        builder.filter_entry(|entry| !should_skip_directory(entry));
        let walker = builder.build();

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
        let returned = entries.len().min(limit);
        let truncated = match_count > limit;
        let result = entries
            .iter()
            .take(limit)
            .map(|(p, _)| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult::from_call(call, result).with_details(serde_json::json!({
            "pattern": pattern,
            "matches": match_count,
            "returned": returned,
            "limit": limit,
            "truncated": truncated,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_tool_definition_mentions_gitignore_and_common_ignores() {
        let definition = GlobTool.definition();

        assert!(definition.description.contains(".gitignore"));
    }
}
