use std::io::{BufRead, BufReader, Read as IoRead};
use std::path::Path;
use std::process::{Command, Stdio};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolOutputStream, ToolResult,
};

pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".into(),
            description: "Execute a bash command".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        }
    }

    fn call(
        &self,
        call: &ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let command = call.str_arg("command")?;
        let cwd = context.workspace_root.as_deref().unwrap_or_else(|| Path::new("."));

        let mut child = Command::new("bash")
            .arg("-c")
            .arg(&command)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CoreError::new(format!("failed to spawn bash: {e}")))?;

        // Read stderr in a background thread to avoid pipe deadlocks.
        let stderr_pipe = child.stderr.take();
        let stderr_thread = std::thread::spawn(move || -> String {
            let Some(pipe) = stderr_pipe else { return String::new() };
            let mut buf = String::new();
            let _ = BufReader::new(pipe).read_to_string(&mut buf);
            buf
        });

        // Stream stdout line-by-line, checking abort between lines.
        let mut stdout_buf = String::new();
        if let Some(pipe) = child.stdout.take() {
            let reader = BufReader::new(pipe);
            for line in reader.lines() {
                if context.abort.is_aborted() {
                    let _ = child.kill();
                    return Ok(ToolResult::from_call(call, "[aborted]"));
                }
                match line {
                    Ok(text) => {
                        let chunk = format!("{text}\n");
                        output(ToolOutputDelta {
                            stream: ToolOutputStream::Stdout,
                            text: chunk.clone(),
                        });
                        stdout_buf.push_str(&chunk);
                    }
                    Err(_) => break,
                }
            }
        }

        if context.abort.is_aborted() {
            let _ = child.kill();
            return Ok(ToolResult::from_call(call, "[aborted]"));
        }

        let stderr_buf = stderr_thread.join().unwrap_or_default();
        if !stderr_buf.is_empty() {
            output(ToolOutputDelta {
                stream: ToolOutputStream::Stderr,
                text: stderr_buf.clone(),
            });
        }

        let status = child
            .wait()
            .map_err(|e| CoreError::new(format!("bash wait failed: {e}")))?;
        let exit_code = status.code().unwrap_or(-1);

        let mut result_text = stdout_buf;
        if !stderr_buf.is_empty() {
            result_text.push_str(&stderr_buf);
        }
        if !status.success() {
            result_text.push_str(&format!("\n[exit code: {exit_code}]"));
        }

        Ok(ToolResult::from_call(call, result_text))
    }
}
