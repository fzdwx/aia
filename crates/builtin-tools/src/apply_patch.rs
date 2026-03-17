use agent_core::{
    CoreError, Tool, ToolArgsSchema, ToolCall, ToolDefinition, ToolExecutionContext,
    ToolOutputDelta, ToolResult,
};
use agent_prompts::tool_descriptions::apply_patch_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

const BEGIN_PATCH_MARKER: &str = "*** Begin Patch";
const END_PATCH_MARKER: &str = "*** End Patch";
const UPDATE_FILE_PREFIX: &str = "*** Update File: ";
const ADD_FILE_PREFIX: &str = "*** Add File: ";
const DELETE_FILE_PREFIX: &str = "*** Delete File: ";
const MOVE_TO_PREFIX: &str = "*** Move to: ";
const NO_NEWLINE_MARKER: &str = r"\ No newline at end of file";

pub struct ApplyPatchTool;

#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[serde(deny_unknown_fields)]
#[tool_schema(min_properties = 1)]
pub(crate) struct ApplyPatchToolArgs {
    #[tool_schema(description = "The full patch text in apply_patch format")]
    patch: Option<String>,
    #[serde(rename = "patchText")]
    #[tool_schema(description = "Alias for patch; the full patch text in apply_patch format")]
    patch_text: Option<String>,
}

impl ApplyPatchToolArgs {
    fn patch_text(self) -> Result<String, CoreError> {
        match (self.patch, self.patch_text) {
            (Some(patch), None) => Ok(patch),
            (None, Some(patch_text)) => Ok(patch_text),
            (Some(patch), Some(patch_text)) if patch == patch_text => Ok(patch),
            (Some(_), Some(_)) => {
                Err(CoreError::new("patch and patchText must match when both are provided"))
            }
            (None, None) => Err(CoreError::new("either patch or patchText must be provided")),
        }
    }
}

#[derive(Debug)]
struct ParsedPatch {
    operations: Vec<PatchOperation>,
}

#[derive(Debug)]
enum PatchOperation {
    Update { path: String, move_to: Option<String>, hunks: Vec<PatchHunk> },
    Add { path: String, content: String },
    Delete { path: String },
}

#[derive(Debug)]
struct PatchHunk {
    old_text: String,
    new_text: String,
    added_lines: usize,
    removed_lines: usize,
    raw_lines: Vec<String>,
}

#[derive(Debug)]
struct PatchSummary {
    updated_files: usize,
    added_files: usize,
    deleted_files: usize,
    moved_files: usize,
    lines_added: usize,
    lines_removed: usize,
    operations: Vec<serde_json::Value>,
    files: Vec<PatchFileDetail>,
}

#[derive(Debug, Serialize)]
struct PatchFileDetail {
    kind: PatchFileKind,
    file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    move_to: Option<String>,
    added: usize,
    removed: usize,
    before: String,
    after: String,
    patch: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum PatchFileKind {
    Add,
    Update,
    Delete,
    Move,
}

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), apply_patch_tool_description())
            .with_parameters_schema::<ApplyPatchToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let patch = call.parse_arguments::<ApplyPatchToolArgs>()?.patch_text()?;
        let parsed = parse_apply_patch(&patch)?;
        let summary = apply_parsed_patch(context, parsed).await?;
        let total_files = summary.updated_files
            + summary.added_files
            + summary.deleted_files
            + summary.moved_files;
        let noun = if total_files == 1 { "file" } else { "files" };

        Ok(ToolResult::from_call(call, format!("Applied patch to {total_files} {noun}"))
            .with_details(json!({
                "files_updated": summary.updated_files,
                "files_added": summary.added_files,
                "files_deleted": summary.deleted_files,
                "files_moved": summary.moved_files,
                "lines_added": summary.lines_added,
                "lines_removed": summary.lines_removed,
                "operations": summary.operations,
                "files": summary.files,
            })))
    }
}

fn parse_apply_patch(patch: &str) -> Result<ParsedPatch, CoreError> {
    let lines = patch.lines().collect::<Vec<_>>();
    if lines.first().copied() != Some(BEGIN_PATCH_MARKER) {
        return Err(CoreError::new("patch must start with *** Begin Patch"));
    }
    if lines.last().copied() != Some(END_PATCH_MARKER) {
        return Err(CoreError::new("patch must end with *** End Patch"));
    }

    let mut index = 1;
    let mut operations = Vec::new();
    while index < lines.len() - 1 {
        let line = lines[index];
        if line.is_empty() {
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix(UPDATE_FILE_PREFIX) {
            index += 1;
            let mut move_to = None;
            if index < lines.len() - 1
                && let Some(target_path) = lines[index].strip_prefix(MOVE_TO_PREFIX)
            {
                move_to = Some(target_path.to_owned());
                index += 1;
            }

            let mut hunk_lines = Vec::new();
            while index < lines.len() - 1 && !is_patch_operation_marker(lines[index]) {
                hunk_lines.push(lines[index]);
                index += 1;
            }
            let hunks = parse_update_hunks(&hunk_lines, path, move_to.is_some())?;
            operations.push(PatchOperation::Update { path: path.into(), move_to, hunks });
            continue;
        }

        if let Some(path) = line.strip_prefix(ADD_FILE_PREFIX) {
            index += 1;
            let mut content_lines = Vec::new();
            while index < lines.len() - 1 && !is_patch_operation_marker(lines[index]) {
                let current = lines[index];
                if current == NO_NEWLINE_MARKER {
                    index += 1;
                    continue;
                }
                let Some(added_line) = current.strip_prefix('+') else {
                    return Err(CoreError::new(format!(
                        "invalid add-file patch line for {path}: {current}"
                    )));
                };
                content_lines.push(added_line.to_owned());
                index += 1;
            }
            operations.push(PatchOperation::Add {
                path: path.into(),
                content: patch_text_from_lines(&content_lines),
            });
            continue;
        }

        if let Some(path) = line.strip_prefix(DELETE_FILE_PREFIX) {
            operations.push(PatchOperation::Delete { path: path.into() });
            index += 1;
            continue;
        }

        return Err(CoreError::new(format!("unsupported patch line: {line}")));
    }

    if operations.is_empty() {
        return Err(CoreError::new("patch did not contain any file operations"));
    }

    Ok(ParsedPatch { operations })
}

fn parse_update_hunks(
    lines: &[&str],
    path: &str,
    allow_empty: bool,
) -> Result<Vec<PatchHunk>, CoreError> {
    let mut hunks = Vec::new();
    let mut current = Vec::new();

    for line in lines {
        if line.starts_with("@@") {
            if !current.is_empty() {
                hunks.push(build_hunk(&current, path)?);
                current.clear();
            }
            continue;
        }

        if *line == NO_NEWLINE_MARKER {
            continue;
        }

        let prefix = line.chars().next().unwrap_or('\0');
        if !matches!(prefix, ' ' | '+' | '-') {
            return Err(CoreError::new(format!("invalid patch hunk line for {path}: {line}")));
        }
        current.push((*line).to_owned());
    }

    if !current.is_empty() {
        hunks.push(build_hunk(&current, path)?);
    }

    if hunks.is_empty() && !allow_empty {
        return Err(CoreError::new(format!("update patch for {path} contained no hunks")));
    }

    Ok(hunks)
}

fn build_hunk(lines: &[String], path: &str) -> Result<PatchHunk, CoreError> {
    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();
    let mut added_lines = 0;
    let mut removed_lines = 0;

    for line in lines {
        let prefix = line.chars().next().unwrap_or('\0');
        let text = line.get(1..).unwrap_or("");
        match prefix {
            ' ' => {
                old_lines.push(text.to_owned());
                new_lines.push(text.to_owned());
            }
            '-' => {
                old_lines.push(text.to_owned());
                removed_lines += 1;
            }
            '+' => {
                new_lines.push(text.to_owned());
                added_lines += 1;
            }
            _ => {
                return Err(CoreError::new(format!("invalid patch hunk line for {path}: {line}")));
            }
        }
    }

    if old_lines.is_empty() {
        return Err(CoreError::new(format!(
            "patch hunk for {path} must include context or removed lines"
        )));
    }

    Ok(PatchHunk {
        old_text: patch_text_from_lines(&old_lines),
        new_text: patch_text_from_lines(&new_lines),
        added_lines,
        removed_lines,
        raw_lines: lines.to_vec(),
    })
}

fn is_patch_operation_marker(line: &str) -> bool {
    line.starts_with(UPDATE_FILE_PREFIX)
        || line.starts_with(ADD_FILE_PREFIX)
        || line.starts_with(DELETE_FILE_PREFIX)
        || line == END_PATCH_MARKER
}

fn patch_text_from_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut text = lines.join("\n");
    text.push('\n');
    text
}

async fn apply_parsed_patch(
    context: &ToolExecutionContext,
    parsed: ParsedPatch,
) -> Result<PatchSummary, CoreError> {
    let mut summary = PatchSummary {
        updated_files: 0,
        added_files: 0,
        deleted_files: 0,
        moved_files: 0,
        lines_added: 0,
        lines_removed: 0,
        operations: Vec::new(),
        files: Vec::new(),
    };

    for operation in parsed.operations {
        match operation {
            PatchOperation::Update { path, move_to, hunks } => {
                let resolved_path = context.resolve_path(&path);
                let original_content =
                    tokio::fs::read_to_string(&resolved_path).await.map_err(|error| {
                        CoreError::new(format!(
                            "failed to read {}: {error}",
                            resolved_path.display()
                        ))
                    })?;
                let mut content = original_content.clone();

                let mut file_added_lines = 0;
                let mut file_removed_lines = 0;
                let display_path = resolved_path.display().to_string();
                for hunk in &hunks {
                    let (start, end) =
                        find_unique_patch_target(&content, &display_path, &hunk.old_text)?;
                    content.replace_range(start..end, &hunk.new_text);
                    file_added_lines += hunk.added_lines;
                    file_removed_lines += hunk.removed_lines;
                }

                let per_file_patch = render_update_patch(&path, move_to.as_deref(), &hunks);
                if let Some(target_path) = move_to {
                    let resolved_target = context.resolve_path(&target_path);
                    if resolved_target != resolved_path {
                        if tokio::fs::try_exists(&resolved_target).await.map_err(|error| {
                            CoreError::new(format!(
                                "failed to check whether {} exists: {error}",
                                resolved_target.display()
                            ))
                        })? {
                            return Err(CoreError::new(format!(
                                "cannot move to {}; file already exists",
                                resolved_target.display()
                            )));
                        }
                        if let Some(parent) = resolved_target.parent() {
                            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                                CoreError::new(format!(
                                    "failed to create directory {}: {error}",
                                    parent.display()
                                ))
                            })?;
                        }
                        tokio::fs::write(&resolved_target, &content).await.map_err(|error| {
                            CoreError::new(format!(
                                "failed to write {}: {error}",
                                resolved_target.display()
                            ))
                        })?;
                        tokio::fs::remove_file(&resolved_path).await.map_err(|error| {
                            CoreError::new(format!(
                                "failed to delete {}: {error}",
                                resolved_path.display()
                            ))
                        })?;

                        let target_display = resolved_target.display().to_string();
                        summary.moved_files += 1;
                        summary.lines_added += file_added_lines;
                        summary.lines_removed += file_removed_lines;
                        summary.operations.push(json!({
                            "kind": "move",
                            "file_path": display_path,
                            "move_to": target_display,
                            "added": file_added_lines,
                            "removed": file_removed_lines,
                        }));
                        summary.files.push(PatchFileDetail {
                            kind: PatchFileKind::Move,
                            file_path: display_path,
                            move_to: Some(target_display),
                            added: file_added_lines,
                            removed: file_removed_lines,
                            before: original_content,
                            after: content,
                            patch: per_file_patch,
                        });
                        continue;
                    }
                }

                tokio::fs::write(&resolved_path, &content).await.map_err(|error| {
                    CoreError::new(format!("failed to write {}: {error}", resolved_path.display()))
                })?;

                summary.updated_files += 1;
                summary.lines_added += file_added_lines;
                summary.lines_removed += file_removed_lines;
                summary.operations.push(json!({
                    "kind": "update",
                    "file_path": display_path,
                    "added": file_added_lines,
                    "removed": file_removed_lines,
                }));
                summary.files.push(PatchFileDetail {
                    kind: PatchFileKind::Update,
                    file_path: display_path,
                    move_to: None,
                    added: file_added_lines,
                    removed: file_removed_lines,
                    before: original_content,
                    after: content,
                    patch: per_file_patch,
                });
            }
            PatchOperation::Add { path, content } => {
                let resolved_path = context.resolve_path(&path);
                if tokio::fs::try_exists(&resolved_path).await.map_err(|error| {
                    CoreError::new(format!(
                        "failed to check whether {} exists: {error}",
                        resolved_path.display()
                    ))
                })? {
                    return Err(CoreError::new(format!(
                        "cannot add {}; file already exists",
                        resolved_path.display()
                    )));
                }

                if let Some(parent) = resolved_path.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|error| {
                        CoreError::new(format!(
                            "failed to create directory {}: {error}",
                            parent.display()
                        ))
                    })?;
                }

                tokio::fs::write(&resolved_path, &content).await.map_err(|error| {
                    CoreError::new(format!("failed to write {}: {error}", resolved_path.display()))
                })?;

                let added_lines = line_count(&content);
                let display_path = resolved_path.display().to_string();
                summary.added_files += 1;
                summary.lines_added += added_lines;
                summary.operations.push(json!({
                    "kind": "add",
                    "file_path": display_path,
                    "added": added_lines,
                    "removed": 0,
                }));
                summary.files.push(PatchFileDetail {
                    kind: PatchFileKind::Add,
                    file_path: display_path,
                    move_to: None,
                    added: added_lines,
                    removed: 0,
                    before: String::new(),
                    after: content.clone(),
                    patch: render_add_patch(&path, &content),
                });
            }
            PatchOperation::Delete { path } => {
                let resolved_path = context.resolve_path(&path);
                let existing_content =
                    tokio::fs::read_to_string(&resolved_path).await.map_err(|error| {
                        CoreError::new(format!(
                            "failed to read {}: {error}",
                            resolved_path.display()
                        ))
                    })?;
                tokio::fs::remove_file(&resolved_path).await.map_err(|error| {
                    CoreError::new(format!("failed to delete {}: {error}", resolved_path.display()))
                })?;

                let removed_lines = line_count(&existing_content);
                let display_path = resolved_path.display().to_string();
                summary.deleted_files += 1;
                summary.lines_removed += removed_lines;
                summary.operations.push(json!({
                    "kind": "delete",
                    "file_path": display_path,
                    "added": 0,
                    "removed": removed_lines,
                }));
                summary.files.push(PatchFileDetail {
                    kind: PatchFileKind::Delete,
                    file_path: display_path,
                    move_to: None,
                    added: 0,
                    removed: removed_lines,
                    before: existing_content,
                    after: String::new(),
                    patch: render_delete_patch(&path),
                });
            }
        }
    }

    Ok(summary)
}

fn render_update_patch(path: &str, move_to: Option<&str>, hunks: &[PatchHunk]) -> String {
    let mut lines = vec![BEGIN_PATCH_MARKER.to_owned(), format!("{UPDATE_FILE_PREFIX}{path}")];
    if let Some(move_to) = move_to {
        lines.push(format!("{MOVE_TO_PREFIX}{move_to}"));
    }
    for hunk in hunks {
        lines.push("@@".to_owned());
        lines.extend(hunk.raw_lines.iter().cloned());
    }
    lines.push(END_PATCH_MARKER.to_owned());
    lines.join("\n")
}

fn render_add_patch(path: &str, content: &str) -> String {
    let mut lines = vec![BEGIN_PATCH_MARKER.to_owned(), format!("{ADD_FILE_PREFIX}{path}")];
    for line in content.lines() {
        lines.push(format!("+{line}"));
    }
    lines.push(END_PATCH_MARKER.to_owned());
    lines.join("\n")
}

fn render_delete_patch(path: &str) -> String {
    [
        BEGIN_PATCH_MARKER.to_owned(),
        format!("{DELETE_FILE_PREFIX}{path}"),
        END_PATCH_MARKER.to_owned(),
    ]
    .join("\n")
}

fn find_unique_patch_target(
    content: &str,
    display_path: &str,
    old_text: &str,
) -> Result<(usize, usize), CoreError> {
    for candidate in patch_match_candidates(old_text) {
        let matches = content.match_indices(candidate).map(|(index, _)| index).collect::<Vec<_>>();

        match matches.len() {
            0 => continue,
            1 => {
                let start = matches[0];
                return Ok((start, start + candidate.len()));
            }
            count => {
                return Err(CoreError::new(format!(
                    "patch hunk matched {count} locations in {display_path}; provide more context"
                )));
            }
        }
    }

    Err(CoreError::new(format!("patch hunk not found in {display_path}")))
}

fn patch_match_candidates(old_text: &str) -> Vec<&str> {
    let mut candidates = vec![old_text];
    if let Some(trimmed) = old_text.strip_suffix('\n')
        && !trimmed.is_empty()
    {
        candidates.push(trimmed);
    }
    candidates
}

fn line_count(text: &str) -> usize {
    if text.is_empty() { 0 } else { text.lines().count() }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs,
        path::{Path, PathBuf},
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext};

    use super::ApplyPatchTool;

    #[test]
    fn apply_patch_tool_definition_exposes_flat_object_schema() {
        let definition = ApplyPatchTool.definition();

        assert_eq!(definition.parameters["type"], "object");
        assert!(definition.parameters.get("$defs").is_none());
        assert!(definition.parameters.get("anyOf").is_none());
        assert_eq!(definition.parameters["properties"]["patch"]["type"], "string");
        assert_eq!(definition.parameters["properties"]["patchText"]["type"], "string");
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Result<Self, Box<dyn Error>> {
            let unique =
                SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
            let path = std::env::temp_dir()
                .join(format!("aia-builtin-apply-patch-tests-{}-{unique}", process::id()));
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

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_updates_file() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("notes.txt");
        fs::write(&path, "before\nalpha\nbeta\nafter\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patch": "*** Begin Patch\n*** Update File: notes.txt\n@@\n before\n alpha\n-beta\n+gamma\n after\n*** End Patch"
        }));

        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert_eq!(fs::read_to_string(&path)?, "before\nalpha\ngamma\nafter\n");
        assert_eq!(result.content, "Applied patch to 1 file");
        let details = match result.details {
            Some(details) => details,
            None => return Err("apply_patch result should include details".into()),
        };
        assert_eq!(details["files_updated"], 1);
        assert_eq!(details["lines_added"], 1);
        assert_eq!(details["lines_removed"], 1);
        assert_eq!(details["files"][0]["before"], "before\nalpha\nbeta\nafter\n");
        assert_eq!(details["files"][0]["after"], "before\nalpha\ngamma\nafter\n");
        assert_eq!(
            details["files"][0]["patch"],
            "*** Begin Patch\n*** Update File: notes.txt\n@@\n before\n alpha\n-beta\n+gamma\n after\n*** End Patch"
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_accepts_patch_text_alias() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("notes.txt");
        fs::write(&path, "alpha\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patchText": "*** Begin Patch\n*** Update File: notes.txt\n@@\n-alpha\n+beta\n*** End Patch"
        }));

        tool.call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert_eq!(fs::read_to_string(&path)?, "beta\n");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_rejects_conflicting_patch_and_patch_text()
    -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("notes.txt");
        fs::write(&path, "alpha\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patch": "*** Begin Patch\n*** Update File: notes.txt\n@@\n-alpha\n+beta\n*** End Patch",
            "patchText": "*** Begin Patch\n*** Update File: notes.txt\n@@\n-alpha\n+gamma\n*** End Patch"
        }));

        let error = match tool.call(&call, &mut |_| {}, &test_context(dir.path())).await {
            Ok(_) => return Err("apply_patch should reject conflicting patch inputs".into()),
            Err(error) => error,
        };

        assert!(error.to_string().contains("patch and patchText must match"));
        assert_eq!(fs::read_to_string(&path)?, "alpha\n");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_adds_and_deletes_files() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let deleted = dir.path().join("old.txt");
        fs::write(&deleted, "legacy\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patch": "*** Begin Patch\n*** Add File: nested/new.txt\n+first\n+second\n*** Delete File: old.txt\n*** End Patch"
        }));

        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert_eq!(fs::read_to_string(dir.path().join("nested/new.txt"))?, "first\nsecond\n");
        assert!(!deleted.exists());
        let details = match result.details {
            Some(details) => details,
            None => return Err("apply_patch result should include details".into()),
        };
        assert_eq!(details["files_added"], 1);
        assert_eq!(details["files_deleted"], 1);
        assert_eq!(details["files"][0]["kind"], "add");
        assert_eq!(details["files"][1]["kind"], "delete");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_moves_file_without_content_changes() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let source = dir.path().join("old.txt");
        let target = dir.path().join("nested/new.txt");
        fs::write(&source, "legacy\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patch": "*** Begin Patch\n*** Update File: old.txt\n*** Move to: nested/new.txt\n*** End Patch"
        }));

        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert!(!source.exists());
        assert_eq!(fs::read_to_string(&target)?, "legacy\n");
        let details = match result.details {
            Some(details) => details,
            None => return Err("apply_patch result should include details".into()),
        };
        assert_eq!(details["files_moved"], 1);
        assert_eq!(details["operations"][0]["kind"], "move");
        assert_eq!(details["files"][0]["move_to"], target.display().to_string());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_moves_and_updates_file() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let source = dir.path().join("old.txt");
        let target = dir.path().join("nested/new.txt");
        fs::write(&source, "before\nalpha\nbeta\nafter\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patch": "*** Begin Patch\n*** Update File: old.txt\n*** Move to: nested/new.txt\n@@\n before\n alpha\n-beta\n+gamma\n after\n*** End Patch"
        }));

        tool.call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        assert!(!source.exists());
        assert_eq!(fs::read_to_string(&target)?, "before\nalpha\ngamma\nafter\n");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_patch_tool_rejects_ambiguous_hunk() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("duplicate.txt");
        fs::write(&path, "target\nother\ntarget\n")?;

        let tool = ApplyPatchTool;
        let call = ToolCall::new("apply_patch").with_arguments_value(serde_json::json!({
            "patch": "*** Begin Patch\n*** Update File: duplicate.txt\n@@\n-target\n+replacement\n*** End Patch"
        }));

        let error = match tool.call(&call, &mut |_| {}, &test_context(dir.path())).await {
            Ok(_) => return Err("apply_patch should reject ambiguous hunks".into()),
            Err(error) => error,
        };

        assert!(error.to_string().contains("matched 2 locations"));
        assert_eq!(fs::read_to_string(&path)?, "target\nother\ntarget\n");
        Ok(())
    }
}
