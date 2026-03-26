use std::{
    error::Error,
    fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext};

use super::WriteTool;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Result<Self, Box<dyn Error>> {
        let unique =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
        let path = std::env::temp_dir()
            .join(format!("aia-builtin-write-tests-{}-{unique}", process::id()));
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
    }
}

#[tokio::test(flavor = "current_thread")]
async fn write_tool_creates_parent_directories_and_reports_line_count() -> Result<(), Box<dyn Error>>
{
    let dir = TestDir::new()?;
    let tool = WriteTool;
    let call = ToolCall::new("write").with_arguments_value(serde_json::json!({
        "file_path": "nested/notes.txt",
        "content": "alpha\nbeta\n"
    }));

    let result = tool
        .call(&call, &mut |_| {}, &test_context(dir.path()))
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    let written_path = dir.path().join("nested/notes.txt");
    let stored = fs::read_to_string(&written_path)?;
    assert_eq!(stored, "alpha\nbeta\n");
    assert_eq!(result.content, format!("Wrote 11 bytes to {}", written_path.display()));

    let details = match result.details {
        Some(details) => details,
        None => return Err("write result should include details".into()),
    };
    assert_eq!(details["file_path"], written_path.display().to_string());
    assert_eq!(details["lines"], 2);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn write_tool_writes_large_content_without_truncation() -> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    let content = "abc123\n".repeat(4096);
    let expected_bytes = content.len();
    let tool = WriteTool;
    let call = ToolCall::new("write").with_arguments_value(serde_json::json!({
        "file_path": "large.txt",
        "content": content,
    }));

    tool.call(&call, &mut |_| {}, &test_context(dir.path()))
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    let path = dir.path().join("large.txt");
    let mut file = File::open(path)?;
    let mut stored = String::new();
    file.read_to_string(&mut stored)?;
    assert_eq!(stored.len(), expected_bytes);
    assert_eq!(stored, "abc123\n".repeat(4096));
    Ok(())
}
