use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext};

use super::ReadTool;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Result<Self, Box<dyn Error>> {
        let unique =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
        let path =
            std::env::temp_dir().join(format!("aia-builtin-read-tests-{}-{unique}", process::id()));
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
async fn read_tool_reads_large_file_window_with_line_numbers() -> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    let path = dir.path().join("large.txt");
    let content = (1..=2500).map(|index| format!("line {index}")).collect::<Vec<_>>().join("\n");
    fs::write(&path, content)?;

    let tool = ReadTool;
    let call = ToolCall::new("read").with_arguments_value(serde_json::json!({
        "file_path": "large.txt",
        "offset": 1995,
        "limit": 3
    }));
    let result = tool
        .call(&call, &mut |_| {}, &test_context(dir.path()))
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    assert_eq!(result.content, "  1996\tline 1996\n  1997\tline 1997\n  1998\tline 1998");

    let details = match result.details {
        Some(details) => details,
        None => return Err("read result should include details".into()),
    };
    assert_eq!(details["lines_read"], 3);
    assert_eq!(details["total_lines"], 2500);
    assert_eq!(details["file_path"], path.display().to_string());
    assert_eq!(details["is_image"], false);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn read_tool_returns_image_as_base64_data_url() -> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    let path = dir.path().join("pixel.png");
    let png_bytes = [
        0x89_u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00,
        0xb5, 0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63,
        0xfc, 0xff, 0x1f, 0x00, 0x03, 0x03, 0x01, 0xff, 0xa5, 0xf7, 0xa9, 0xb5, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    fs::write(&path, png_bytes)?;

    let tool = ReadTool;
    let call =
        ToolCall::new("read").with_arguments_value(serde_json::json!({ "file_path": "pixel.png" }));
    let result = tool
        .call(&call, &mut |_| {}, &test_context(dir.path()))
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    assert!(result.content.starts_with("data:image/png;base64,"));

    let details = match result.details {
        Some(details) => details,
        None => return Err("read result should include details".into()),
    };
    assert_eq!(details["file_path"], path.display().to_string());
    assert_eq!(details["is_image"], true);
    assert_eq!(details["mime_type"], "image/png");
    assert_eq!(details["encoding"], "data_url");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn read_tool_reports_binary_file_as_non_utf8_text() -> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    let path = dir.path().join("binary.bin");
    fs::write(&path, [0xff_u8, 0xfe_u8, 0x00_u8, 0x61_u8])?;

    let tool = ReadTool;
    let call = ToolCall::new("read")
        .with_arguments_value(serde_json::json!({ "file_path": "binary.bin" }));
    let error = match tool.call(&call, &mut |_| {}, &test_context(dir.path())).await {
        Ok(_) => return Err("read tool should reject non-UTF-8 files".into()),
        Err(error) => error,
    };

    assert!(error.to_string().contains("binary.bin"));
    assert!(error.to_string().contains("not valid UTF-8 text"));
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn read_tool_surfaces_permission_denied_errors() -> Result<(), Box<dyn Error>> {
    use std::{fs::Permissions, os::unix::fs::PermissionsExt};

    struct PermissionReset {
        path: PathBuf,
        original: Permissions,
    }

    impl Drop for PermissionReset {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.path, self.original.clone());
        }
    }

    let dir = TestDir::new()?;
    let path = dir.path().join("secret.txt");
    fs::write(&path, "secret")?;
    let original = fs::metadata(&path)?.permissions();
    fs::set_permissions(&path, Permissions::from_mode(0o000))?;
    let _reset = PermissionReset { path: path.clone(), original };

    if fs::read_to_string(&path).is_ok() {
        return Ok(());
    }

    let tool = ReadTool;
    let call = ToolCall::new("read")
        .with_arguments_value(serde_json::json!({ "file_path": "secret.txt" }));
    let error = match tool.call(&call, &mut |_| {}, &test_context(dir.path())).await {
        Ok(_) => return Err("read tool should surface permission errors".into()),
        Err(error) => error,
    };

    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("secret.txt"));
    Ok(())
}
