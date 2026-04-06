use std::path::PathBuf;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolOutputStream, ToolResult,
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
        "Grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), grep_tool_description())
            .with_parameters_schema::<GrepToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
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

        match run_grep_search(pattern.clone(), base, glob_filter, limit, abort, output).await? {
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
    output: &mut (dyn FnMut(ToolOutputDelta) + Send),
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
    let mut emitted = 0usize;
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
            let rendered = path.display().to_string();
            if emitted < limit {
                let mut text = String::new();
                if emitted > 0 {
                    text.push('\n');
                }
                text.push_str(&rendered);
                output(ToolOutputDelta { stream: ToolOutputStream::Stdout, text });
                emitted += 1;
            }
            matched_files.push(rendered);
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
#[path = "../tests/grep/mod.rs"]
mod tests;
