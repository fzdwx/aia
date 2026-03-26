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
        session_id: None,
        workspace_root: Some(workspace_root.to_path_buf()),
        abort: AbortSignal::new(),
        runtime: None,
        runtime_host: None,
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
async fn apply_patch_tool_rejects_conflicting_patch_and_patch_text() -> Result<(), Box<dyn Error>> {
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
