use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agent_core::{
    AbortSignal, Tool, ToolCall, ToolExecutionContext, ToolOutputDelta, ToolOutputStream,
};

use super::GlobTool;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Result<Self, Box<dyn Error>> {
        let unique =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
        let path =
            std::env::temp_dir().join(format!("aia-builtin-glob-tests-{}-{unique}", process::id()));
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
fn glob_tool_definition_mentions_gitignore_and_common_ignores() {
    let definition = GlobTool.definition();

    assert!(definition.description.contains(".gitignore"));
}

#[tokio::test(flavor = "current_thread")]
async fn glob_tool_respects_gitignore_and_skips_common_directories() -> Result<(), Box<dyn Error>> {
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

#[tokio::test(flavor = "current_thread")]
async fn glob_tool_emits_incremental_output_deltas() -> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    fs::write(dir.path().join("file-a.rs"), "fn a() {}\n")?;
    fs::write(dir.path().join("file-b.rs"), "fn b() {}\n")?;

    let tool = GlobTool;
    let call = ToolCall::new("glob").with_arguments_value(serde_json::json!({
        "pattern": "**/*.rs"
    }));
    let mut deltas: Vec<ToolOutputDelta> = Vec::new();
    let result = tool
        .call(&call, &mut |delta| deltas.push(delta), &test_context(dir.path()))
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    let stdout = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
        .map(|delta| delta.text.as_str())
        .collect::<String>();

    assert!(!stdout.is_empty());
    assert_eq!(stdout.trim_end(), result.content.trim_end());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn glob_tool_returns_aborted_result_when_signal_is_pre_cancelled()
-> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    fs::write(dir.path().join("kept.rs"), "fn kept() {}\n")?;
    let abort = AbortSignal::new();
    abort.abort();

    let tool = GlobTool;
    let call = ToolCall::new("glob").with_arguments_value(serde_json::json!({
        "pattern": "**/*.rs"
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
    let details = result.details.ok_or("glob aborted result should include details")?;
    assert_eq!(details["aborted"], true);
    Ok(())
}
