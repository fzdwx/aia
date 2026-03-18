use std::path::PathBuf;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::grep_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::walk::collect_candidate_files;

pub struct GrepTool;
const DEFAULT_MATCH_LIMIT: usize = 200;
const MAX_MATCH_LIMIT: usize = 1000;

struct GrepSearchResult {
    content: String,
    match_count: usize,
    returned: usize,
    truncated: bool,
}

enum GrepSearchOutcome {
    Completed(GrepSearchResult),
    Cancelled,
}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrepToolArgs {
    #[tool_schema(description = "Regex pattern to search for")]
    pattern: String,
    #[tool_schema(description = "Directory or file to search in")]
    path: Option<String>,
    #[tool_schema(description = "File glob filter (e.g. *.rs)")]
    glob: Option<String>,
    #[tool_schema(description = "Maximum matched files to return (default 200, max 1000)")]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), grep_tool_description())
            .with_parameters_schema::<GrepToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: GrepToolArgs = call.parse_arguments()?;
        let pattern = args.pattern;
        let limit = args.limit.unwrap_or(DEFAULT_MATCH_LIMIT).min(MAX_MATCH_LIMIT);
        let base = args
            .path
            .as_deref()
            .map(|p| context.resolve_path(p))
            .or_else(|| context.workspace_root.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        let glob_filter = args.glob;
        let abort = context.abort.clone();

        if abort.is_aborted() {
            return Ok(ToolResult::from_call(call, "[aborted]").with_details(serde_json::json!({
                "pattern": pattern,
                "matches": 0,
                "returned": 0,
                "limit": limit,
                "truncated": false,
                "aborted": true,
            })));
        }

        match run_grep_search(pattern.clone(), base, glob_filter, limit, abort).await? {
            GrepSearchOutcome::Completed(result) => Ok(ToolResult::from_call(call, result.content)
                .with_details(serde_json::json!({
                    "pattern": pattern,
                    "matches": result.match_count,
                    "returned": result.returned,
                    "limit": limit,
                    "truncated": result.truncated,
                }))),
            GrepSearchOutcome::Cancelled => Ok(ToolResult::from_call(call, "[aborted]")
                .with_details(serde_json::json!({
                    "pattern": pattern,
                    "matches": 0,
                    "returned": 0,
                    "limit": limit,
                    "truncated": false,
                    "aborted": true,
                }))),
        }
    }
}

async fn run_grep_search(
    pattern: String,
    base: PathBuf,
    glob_filter: Option<String>,
    limit: usize,
    abort: agent_core::AbortSignal,
) -> Result<GrepSearchOutcome, CoreError> {
    let matcher = grep_regex::RegexMatcher::new(&pattern)
        .map_err(|e| CoreError::new(format!("invalid regex pattern: {e}")))?;
    let glob_matcher = glob_filter
        .as_deref()
        .map(globset::Glob::new)
        .transpose()
        .map_err(|e| CoreError::new(format!("invalid glob filter: {e}")))?
        .map(|glob| glob.compile_matcher());

    let mut matched_files: Vec<String> = Vec::new();
    let mut searcher = grep_searcher::Searcher::new();

    let candidates = match collect_candidate_files(&base, &abort, |relative, path| {
        glob_matcher
            .as_ref()
            .is_none_or(|compiled| compiled.is_match(relative) || compiled.is_match(path))
    })
    .await?
    {
        crate::walk::CandidateCollection::Completed(paths) => paths,
        crate::walk::CandidateCollection::Cancelled => return Ok(GrepSearchOutcome::Cancelled),
    };

    for path in candidates {
        if abort.is_aborted() {
            return Ok(GrepSearchOutcome::Cancelled);
        }
        let haystack = match tokio::fs::read(&path).await {
            Ok(haystack) => haystack,
            Err(_) => continue,
        };
        let mut found = false;
        let sink = grep_searcher::sinks::UTF8(|_line_num, _line| {
            found = true;
            Ok(false)
        });
        let _ = searcher.search_slice(&matcher, &haystack, sink);
        if found {
            matched_files.push(path.display().to_string());
        }
    }

    let match_count = matched_files.len();
    let returned = matched_files.len().min(limit);
    let truncated = match_count > limit;
    Ok(GrepSearchOutcome::Completed(GrepSearchResult {
        content: matched_files.into_iter().take(limit).collect::<Vec<_>>().join("\n"),
        match_count,
        returned,
        truncated,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        error::Error,
        fs,
        path::{Path, PathBuf},
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext};

    use super::GrepTool;

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Result<Self, Box<dyn Error>> {
            let unique =
                SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
            let path = std::env::temp_dir()
                .join(format!("aia-builtin-grep-tests-{}-{unique}", process::id()));
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
    fn grep_tool_definition_mentions_gitignore_and_common_ignores() {
        let definition = GrepTool.definition();

        assert!(definition.description.contains(".gitignore"));
        assert!(definition.description.contains("node_modules"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn grep_tool_respects_gitignore_and_glob_filter() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        fs::create_dir_all(dir.path().join(".git"))?;
        fs::write(dir.path().join(".gitignore"), "ignored.rs\n")?;
        fs::write(dir.path().join("keep.rs"), "needle\n")?;
        fs::write(dir.path().join("keep.txt"), "needle\n")?;
        fs::write(dir.path().join("ignored.rs"), "needle\n")?;

        let node_modules = dir.path().join("node_modules");
        fs::create_dir_all(&node_modules)?;
        fs::write(node_modules.join("dep.rs"), "needle\n")?;

        let tool = GrepTool;
        let call = ToolCall::new("grep").with_arguments_value(serde_json::json!({
            "pattern": "needle",
            "glob": "*.rs"
        }));
        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        let paths = result_paths(&result.content);
        assert!(paths.contains(&dir.path().join("keep.rs").display().to_string()));
        assert!(!paths.contains(&dir.path().join("keep.txt").display().to_string()));
        assert!(!paths.contains(&dir.path().join("ignored.rs").display().to_string()));
        assert!(!paths.contains(&node_modules.join("dep.rs").display().to_string()));

        let details = match result.details {
            Some(details) => details,
            None => return Err("grep result should include details".into()),
        };
        assert_eq!(details["matches"], 1);
        assert_eq!(details["returned"], 1);
        assert_eq!(details["truncated"], false);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn grep_tool_skips_binary_files_and_reports_truncation() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        fs::write(dir.path().join("match-a.txt"), "needle\n")?;
        fs::write(dir.path().join("match-b.txt"), "prefix needle suffix\n")?;
        fs::write(
            dir.path().join("binary.bin"),
            [0xff_u8, 0xfe_u8, b'n', b'e', b'e', b'd', b'l', b'e'],
        )?;

        let tool = GrepTool;
        let call = ToolCall::new("grep").with_arguments_value(serde_json::json!({
            "pattern": "needle",
            "limit": 1
        }));
        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        let paths = result_paths(&result.content);
        assert_eq!(paths.len(), 1);
        assert!(!paths.contains(&dir.path().join("binary.bin").display().to_string()));

        let details = match result.details {
            Some(details) => details,
            None => return Err("grep result should include details".into()),
        };
        assert_eq!(details["matches"], 2);
        assert_eq!(details["returned"], 1);
        assert_eq!(details["limit"], 1);
        assert_eq!(details["truncated"], true);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn grep_tool_returns_aborted_result_when_signal_is_pre_cancelled()
    -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        fs::write(dir.path().join("keep.rs"), "needle\n")?;
        let abort = AbortSignal::new();
        abort.abort();

        let tool = GrepTool;
        let call = ToolCall::new("grep").with_arguments_value(serde_json::json!({
            "pattern": "needle"
        }));
        let result = tool
            .call(
                &call,
                &mut |_| {},
                &ToolExecutionContext {
                    run_id: "test-run".into(),
                    workspace_root: Some(dir.path().to_path_buf()),
                    abort,
                    runtime: None,
                },
            )
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert_eq!(result.content, "[aborted]");
        let details = result.details.ok_or("grep aborted result should include details")?;
        assert_eq!(details["aborted"], true);
        Ok(())
    }
}
