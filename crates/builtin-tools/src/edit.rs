use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_prompts::tool_descriptions::edit_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct EditTool;

fn build_edit_diff(old_string: &str, new_string: &str) -> String {
    let removed = old_string.lines().map(|line| format!("-{line}"));
    let added = new_string.lines().map(|line| format!("+{line}"));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditToolArgs {
    file_path: String,
    old_string: String,
    new_string: String,
}

pub(crate) fn edit_tool_parameters() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": {
                "description": "Path to the file to edit",
                "type": "string"
            },
            "old_string": {
                "description": "Exact text to find (must match uniquely)",
                "type": "string"
            },
            "new_string": {
                "description": "Replacement text",
                "type": "string"
            }
        },
        "required": ["file_path", "old_string", "new_string"],
        "additionalProperties": false
    })
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), edit_tool_description())
            .with_parameters_value(edit_tool_parameters())
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: EditToolArgs = call.parse_arguments()?;
        let path = context.resolve_path(&args.file_path);

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| CoreError::new(format!("failed to read {}: {e}", path.display())))?;

        let count = content.matches(&*args.old_string).count();
        match count {
            0 => Err(CoreError::new("old_string not found in file")),
            1 => {
                let new_content = content.replacen(&*args.old_string, &args.new_string, 1);
                tokio::fs::write(&path, &new_content).await.map_err(|e| {
                    CoreError::new(format!("failed to write {}: {e}", path.display()))
                })?;
                let old_lines = args.old_string.lines().count();
                let new_lines = args.new_string.lines().count();
                let diff = build_edit_diff(&args.old_string, &args.new_string);
                let file_path = path.display().to_string();
                Ok(ToolResult::from_call(call, format!("Edited {file_path}")).with_details(
                    serde_json::json!({
                        "file_path": file_path,
                        "added": new_lines,
                        "removed": old_lines,
                        "diff": diff,
                    }),
                ))
            }
            n => Err(CoreError::new(format!(
                "old_string found {n} times; provide more context to make it unique"
            ))),
        }
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

    use super::EditTool;

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Result<Self, Box<dyn Error>> {
            let unique =
                SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
            let path = std::env::temp_dir()
                .join(format!("aia-builtin-edit-tests-{}-{unique}", process::id()));
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
    async fn edit_tool_replaces_unique_multiline_match() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("notes.txt");
        fs::write(&path, "before\nalpha\nbeta\nafter\n")?;

        let tool = EditTool;
        let call = ToolCall::new("edit").with_arguments_value(serde_json::json!({
            "file_path": "notes.txt",
            "old_string": "alpha\nbeta",
            "new_string": "gamma\ndelta\nepsilon"
        }));

        let result = tool
            .call(&call, &mut |_| {}, &test_context(dir.path()))
            .await
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

        let stored = fs::read_to_string(&path)?;
        assert_eq!(stored, "before\ngamma\ndelta\nepsilon\nafter\n");
        let details = match result.details {
            Some(details) => details,
            None => return Err("edit result should include details".into()),
        };
        assert_eq!(details["added"], 3);
        assert_eq!(details["removed"], 2);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn edit_tool_rejects_non_unique_match_without_modifying_file()
    -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = dir.path().join("duplicate.txt");
        fs::write(&path, "target\nother\ntarget\n")?;

        let tool = EditTool;
        let call = ToolCall::new("edit").with_arguments_value(serde_json::json!({
            "file_path": "duplicate.txt",
            "old_string": "target",
            "new_string": "replacement"
        }));

        let error = match tool.call(&call, &mut |_| {}, &test_context(dir.path())).await {
            Ok(_) => return Err("edit should reject non-unique matches".into()),
            Err(error) => error,
        };

        assert!(error.to_string().contains("found 2 times"));
        let stored = fs::read_to_string(&path)?;
        assert_eq!(stored, "target\nother\ntarget\n");
        Ok(())
    }
}
