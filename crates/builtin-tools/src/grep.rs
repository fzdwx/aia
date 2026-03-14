use std::path::PathBuf;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

use crate::should_skip_directory;

pub struct GrepTool;
const DEFAULT_MATCH_LIMIT: usize = 200;
const MAX_MATCH_LIMIT: usize = 1000;

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".into(),
            description:
                "Search file contents with regex (respects .gitignore and skips .git/node_modules/target)"
                    .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search in"
                    },
                    "glob": {
                        "type": "string",
                        "description": "File glob filter (e.g. *.rs)"
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
        let glob_filter = call.opt_str_arg("glob");

        let matcher = grep_regex::RegexMatcher::new(&pattern)
            .map_err(|e| CoreError::new(format!("invalid regex pattern: {e}")))?;

        let mut walker_builder = ignore::WalkBuilder::new(&base);
        walker_builder.filter_entry(|entry| !should_skip_directory(entry));
        if let Some(ref glob_pat) = glob_filter {
            let mut overrides = ignore::overrides::OverrideBuilder::new(&base);
            overrides
                .add(glob_pat)
                .map_err(|e| CoreError::new(format!("invalid glob filter: {e}")))?;
            let built = overrides
                .build()
                .map_err(|e| CoreError::new(format!("failed to build glob filter: {e}")))?;
            walker_builder.overrides(built);
        }

        let mut matched_files: Vec<String> = Vec::new();
        let mut searcher = grep_searcher::Searcher::new();

        for entry in walker_builder.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let path = entry.into_path();
            let mut found = false;
            let sink = grep_searcher::sinks::UTF8(|_line_num, _line| {
                found = true;
                Ok(false)
            });
            // Silently skip files that cannot be searched (binary, permission denied, etc.).
            let _ = searcher.search_path(&matcher, &path, sink);
            if found {
                matched_files.push(path.display().to_string());
            }
        }

        let match_count = matched_files.len();
        let returned = matched_files.len().min(limit);
        let truncated = match_count > limit;
        Ok(ToolResult::from_call(
            call,
            matched_files.into_iter().take(limit).collect::<Vec<_>>().join("\n"),
        )
        .with_details(serde_json::json!({
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
    fn grep_tool_definition_mentions_gitignore_and_common_ignores() {
        let definition = GrepTool.definition();

        assert!(definition.description.contains(".gitignore"));
        assert!(definition.description.contains("node_modules"));
    }
}
