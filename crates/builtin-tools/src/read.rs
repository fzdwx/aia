use std::io::ErrorKind;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_prompts::tool_descriptions::read_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct ReadTool;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadToolArgs {
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

pub(crate) fn read_tool_parameters() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": {
                "description": "Path to the file to read",
                "type": "string"
            },
            "offset": {
                "description": "Starting line number (0-based, default 0)",
                "type": "integer",
                "minimum": 0
            },
            "limit": {
                "description": "Maximum lines to read (default 2000)",
                "type": "integer",
                "minimum": 0
            }
        },
        "required": ["file_path"],
        "additionalProperties": false
    })
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), read_tool_description())
            .with_parameters_value(read_tool_parameters())
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: ReadToolArgs = call.parse_arguments()?;
        let offset = args.offset.unwrap_or(0);
        let limit = args.limit.unwrap_or(2000);
        let path = context.resolve_path(&args.file_path);

        let content =
            tokio::fs::read_to_string(&path).await.map_err(|error| match error.kind() {
                ErrorKind::InvalidData => CoreError::new(format!(
                    "failed to read {}: file is not valid UTF-8 text",
                    path.display()
                )),
                _ => CoreError::new(format!("failed to read {}: {error}", path.display())),
            })?;

        let total_lines = content.lines().count();

        let selected: String = content
            .lines()
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        let lines_read = selected.lines().count();

        Ok(ToolResult::from_call(call, selected).with_details(serde_json::json!({
            "file_path": path.display().to_string(),
            "lines_read": lines_read,
            "total_lines": total_lines,
        })))
    }
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

    use super::ReadTool;

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Result<Self, Box<dyn Error>> {
            let unique =
                SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
            let path = std::env::temp_dir()
                .join(format!("aia-builtin-read-tests-{}-{unique}", process::id()));
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
    async fn read_tool_reads_large_file_window_with_line_numbers() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("large.txt");
        let content =
            (1..=2500).map(|index| format!("line {index}")).collect::<Vec<_>>().join("\n");
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
}
