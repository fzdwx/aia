use std::path::PathBuf;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

pub struct GrepTool;

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".into(),
            description: "Search file contents with regex (respects .gitignore)".into(),
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
        let glob_filter = call.opt_str_arg("glob");

        let matcher = grep_regex::RegexMatcher::new(&pattern)
            .map_err(|e| CoreError::new(format!("invalid regex pattern: {e}")))?;

        let mut walker_builder = ignore::WalkBuilder::new(&base);
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

        Ok(ToolResult::from_call(call, matched_files.join("\n")))
    }
}
