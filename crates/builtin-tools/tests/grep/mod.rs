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
        let path =
            std::env::temp_dir().join(format!("aia-builtin-grep-tests-{}-{unique}", process::id()));
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
                session_id: None,
                workspace_root: Some(dir.path().to_path_buf()),
                abort,
                runtime: None,
                runtime_host: None,
            },
        )
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    assert_eq!(result.content, "[aborted]");
    let details = result.details.ok_or("grep aborted result should include details")?;
    assert_eq!(details["aborted"], true);
    Ok(())
}
