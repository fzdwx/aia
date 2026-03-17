use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use async_trait::async_trait;

use crate::should_skip_directory;

pub struct GlobTool;
const DEFAULT_MATCH_LIMIT: usize = 200;
const MAX_MATCH_LIMIT: usize = 1000;

#[async_trait(?Send)]
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

    async fn call(
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
    use std::{
        collections::BTreeSet,
        error::Error,
        fs,
        path::{Path, PathBuf},
        process,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext};

    use super::GlobTool;

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Result<Self, Box<dyn Error>> {
            let unique =
                SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
            let path = std::env::temp_dir()
                .join(format!("aia-builtin-glob-tests-{}-{unique}", process::id()));
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_context(workspace_root: &Path) -> ToolExecutionContext {
        ToolExecutionContext {
            run_id: "test-run".into(),
            workspace_root: Some(workspace_root.to_path_buf()),
            abort: AbortSignal::new(),
            runtime: None,
        }
    }

    fn result_paths(result: &str) -> BTreeSet<String> {
        result.lines().filter(|line| !line.is_empty()).map(ToOwned::to_owned).collect()
    }

    #[test]
    fn glob_tool_definition_mentions_gitignore_and_common_ignores() {
        let definition = GlobTool.definition();

        assert!(definition.description.contains(".gitignore"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn glob_tool_respects_gitignore_and_skips_common_directories()
    -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        fs::create_dir_all(dir.path().join(".git"))?;
        fs::write(dir.path().join(".gitignore"), "ignored.rs\n")?;
        fs::write(dir.path().join("kept.rs"), "fn kept() {}\n")?;
        fs::write(dir.path().join("ignored.rs"), "fn ignored() {}\n")?;

        let node_modules = dir.path().join("node_modules");
        fs::create_dir_all(&node_modules)?;
        fs::write(node_modules.join("dep.rs"), "fn dep() {}\n")?;

        let target = dir.path().join("target");
        fs::create_dir_all(&target)?;
        fs::write(target.join("generated.rs"), "fn generated() {}\n")?;

        let tool = GlobTool;
        let call = ToolCall::new("glob").with_arguments_value(serde_json::json!({
            "pattern": "**/*.rs"
        }));
        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        let paths = result_paths(&result.content);
        assert!(paths.contains(&dir.path().join("kept.rs").display().to_string()));
        assert!(!paths.contains(&dir.path().join("ignored.rs").display().to_string()));
        assert!(!paths.contains(&node_modules.join("dep.rs").display().to_string()));
        assert!(!paths.contains(&target.join("generated.rs").display().to_string()));

        let details = match result.details {
            Some(details) => details,
            None => return Err("glob result should include details".into()),
        };
        assert_eq!(details["matches"], 1);
        assert_eq!(details["returned"], 1);
        assert_eq!(details["truncated"], false);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn glob_tool_reports_truncation_when_limit_is_smaller_than_matches()
    -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        for index in 0..3 {
            let path = dir.path().join(format!("file-{index}.rs"));
            fs::write(&path, format!("fn file_{index}() {{}}\n"))?;
            std::thread::sleep(Duration::from_millis(2));
        }

        let tool = GlobTool;
        let call = ToolCall::new("glob").with_arguments_value(serde_json::json!({
            "pattern": "**/*.rs",
            "limit": 2
        }));
        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert_eq!(result.content.lines().count(), 2);
        let details = match result.details {
            Some(details) => details,
            None => return Err("glob result should include details".into()),
        };
        assert_eq!(details["matches"], 3);
        assert_eq!(details["returned"], 2);
        assert_eq!(details["limit"], 2);
        assert_eq!(details["truncated"], true);
        Ok(())
    }
}
