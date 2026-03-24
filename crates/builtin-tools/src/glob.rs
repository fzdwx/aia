use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::glob_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::walk::collect_candidate_files;

pub struct GlobTool;
const DEFAULT_MATCH_LIMIT: usize = 200;
const MAX_MATCH_LIMIT: usize = 1000;

struct GlobSearchResult {
    content: String,
    match_count: usize,
    returned: usize,
    truncated: bool,
}

enum GlobSearchOutcome {
    Completed(GlobSearchResult),
    Cancelled,
}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GlobToolArgs {
    #[tool_schema(description = "Glob pattern (e.g. **/*.rs)")]
    pattern: String,
    #[tool_schema(description = "Base directory to search in")]
    path: Option<String>,
    #[tool_schema(description = "Maximum matched files to return (default 200, max 1000)")]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), glob_tool_description())
            .with_parameters_schema::<GlobToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: GlobToolArgs = call.parse_arguments()?;
        let pattern = args.pattern;
        let limit = args.limit.unwrap_or(DEFAULT_MATCH_LIMIT).min(MAX_MATCH_LIMIT);
        let base = args
            .path
            .as_deref()
            .map(|p| context.resolve_path(p))
            .or_else(|| context.workspace_root.clone())
            .unwrap_or_else(|| PathBuf::from("."));
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

        match run_glob_search(pattern.clone(), base, limit, abort).await? {
            GlobSearchOutcome::Completed(result) => Ok(ToolResult::from_call(call, result.content)
                .with_details(serde_json::json!({
                    "pattern": pattern,
                    "matches": result.match_count,
                    "returned": result.returned,
                    "limit": limit,
                    "truncated": result.truncated,
                }))),
            GlobSearchOutcome::Cancelled => Ok(ToolResult::from_call(call, "[aborted]")
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

async fn run_glob_search(
    pattern: String,
    base: PathBuf,
    limit: usize,
    abort: agent_core::AbortSignal,
) -> Result<GlobSearchOutcome, CoreError> {
    let glob = globset::Glob::new(&pattern)
        .map_err(|e| CoreError::new(format!("invalid glob pattern: {e}")))?
        .compile_matcher();

    let candidates = match collect_candidate_files(&base, &abort, |relative, path| {
        glob.is_match(relative) || glob.is_match(path)
    })
    .await?
    {
        crate::walk::CandidateCollection::Completed(paths) => paths,
        crate::walk::CandidateCollection::Cancelled => return Ok(GlobSearchOutcome::Cancelled),
    };

    let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();
    for path in candidates {
        if abort.is_aborted() {
            return Ok(GlobSearchOutcome::Cancelled);
        }
        let mtime = tokio::fs::metadata(&path)
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .unwrap_or(UNIX_EPOCH);
        entries.push((path, mtime));
    }

    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let match_count = entries.len();
    let returned = entries.len().min(limit);
    let truncated = match_count > limit;
    let content = entries
        .iter()
        .take(limit)
        .map(|(p, _)| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(GlobSearchOutcome::Completed(GlobSearchResult { content, match_count, returned, truncated }))
}

#[cfg(test)]
#[path = "../tests/glob/mod.rs"]
mod tests;
