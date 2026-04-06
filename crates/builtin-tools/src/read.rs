use std::{io::ErrorKind, path::Path};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolOutputStream, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::read_tool_description;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::{Deserialize, Serialize};

pub struct ReadTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadToolArgs {
    #[tool_schema(description = "Path to the file to read")]
    file_path: String,
    #[tool_schema(description = "Starting line number (0-based, default 0)")]
    offset: Option<usize>,
    #[tool_schema(description = "Maximum lines to read (default 2000)")]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), read_tool_description())
            .with_parameters_schema::<ReadToolArgs>()
    }

    async fn call(
        &self,
        call: &ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: ReadToolArgs = call.parse_arguments()?;
        let offset = args.offset.unwrap_or(0);
        let limit = args.limit.unwrap_or(2000);
        let path = context.resolve_path(&args.file_path);

        if let Some(mime_type) = detect_image_mime_type(&path) {
            let bytes = tokio::fs::read(&path).await.map_err(|error| {
                CoreError::new(format!("failed to read {}: {error}", path.display()))
            })?;
            let byte_len = bytes.len();
            let data_url = format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes));

            return Ok(ToolResult::from_call(call, data_url.clone()).with_details(
                serde_json::json!({
                    "file_path": path.display().to_string(),
                    "is_image": true,
                    "mime_type": mime_type,
                    "bytes": byte_len,
                    "encoding": "data_url",
                }),
            ));
        }

        let content =
            tokio::fs::read_to_string(&path).await.map_err(|error| match error.kind() {
                ErrorKind::InvalidData => CoreError::new(format!(
                    "failed to read {}: file is not valid UTF-8 text",
                    path.display()
                )),
                _ => CoreError::new(format!("failed to read {}: {error}", path.display())),
            })?;

        let total_lines = content.lines().count();

        let selected_lines = content
            .lines()
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
            .collect::<Vec<_>>();
        for (index, line) in selected_lines.iter().enumerate() {
            let mut text = String::new();
            if index > 0 {
                text.push('\n');
            }
            text.push_str(line);
            output(ToolOutputDelta { stream: ToolOutputStream::Stdout, text });
        }
        let selected = selected_lines.join("\n");

        let lines_read = selected.lines().count();

        Ok(ToolResult::from_call(call, selected).with_details(serde_json::json!({
            "file_path": path.display().to_string(),
            "is_image": false,
            "lines_read": lines_read,
            "total_lines": total_lines,
        })))
    }
}

fn detect_image_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "avif" => Some("image/avif"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/read/mod.rs"]
mod tests;
