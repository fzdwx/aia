use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolOutputStream, ToolResult,
};
use async_trait::async_trait;
use brush_builtins::{BuiltinSet, ShellBuilderExt};
use brush_core::openfiles::OpenFiles;
use brush_core::traps::TrapSignal;

const EMBEDDED_SHELL_NAME: &str = "brush";
const SHELL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub struct ShellTool;

#[async_trait(?Send)]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell".into(),
            description: "Execute a shell command with the embedded brush runtime".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        }
    }

    async fn call(
        &self,
        call: &ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let command = call.str_arg("command")?;
        let cwd = context.workspace_root.as_deref().unwrap_or_else(|| Path::new("."));

        if context.abort.is_aborted() {
            return Ok(ToolResult::from_call(call, "[aborted]"));
        }

        let execution = run_embedded_brush(&command, cwd, &context.abort, output).await?;

        let mut result_text = execution.stdout.clone();
        if !execution.stderr.is_empty() {
            result_text.push_str(&execution.stderr);
        }
        if execution.exit_code != 0 {
            result_text.push_str(&format!("\n[exit code: {}]", execution.exit_code));
        }

        Ok(ToolResult::from_call(call, result_text).with_details(serde_json::json!({
            "command": command,
            "exit_code": execution.exit_code,
            "stdout": execution.stdout,
            "stderr": execution.stderr,
        })))
    }
}

#[derive(Debug)]
struct EmbeddedShellExecution {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

enum ShellEvent {
    Output(ToolOutputDelta),
    StreamClosed(ToolOutputStream),
    Finished(Result<i32, CoreError>),
    ShellReady(tokio::sync::oneshot::Sender<ShellControlMessage>),
}

#[derive(Clone, Copy)]
enum ShellControlMessage {
    Abort,
}

async fn run_embedded_brush(
    command: &str,
    cwd: &Path,
    abort: &agent_core::AbortSignal,
    output: &mut dyn FnMut(ToolOutputDelta),
) -> Result<EmbeddedShellExecution, CoreError> {
    let (stdout_reader, stdout_writer) = std::io::pipe()
        .map_err(|e| CoreError::new(format!("failed to create stdout pipe: {e}")))?;
    let (stderr_reader, stderr_writer) = std::io::pipe()
        .map_err(|e| CoreError::new(format!("failed to create stderr pipe: {e}")))?;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let stdout_handle =
        spawn_pipe_reader(stdout_reader, ToolOutputStream::Stdout, event_tx.clone());
    let stderr_handle =
        spawn_pipe_reader(stderr_reader, ToolOutputStream::Stderr, event_tx.clone());

    let shell_command = command.to_owned();
    let shell_cwd = cwd.to_path_buf();
    let shell_tx = event_tx.clone();
    let shell_handle = thread::spawn(move || {
        let result = run_embedded_brush_in_runtime(
            shell_command,
            shell_cwd,
            stdout_writer,
            stderr_writer,
            shell_tx.clone(),
        );
        let _ = shell_tx.send(ShellEvent::Finished(result));
    });
    drop(event_tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut finished = None;
    let mut control: Option<tokio::sync::oneshot::Sender<ShellControlMessage>> = None;
    let mut abort_sent = false;

    while !(stdout_closed && stderr_closed && finished.is_some()) {
        if abort.is_aborted() && !abort_sent {
            if let Some(control_tx) = control.take() {
                let _ = control_tx.send(ShellControlMessage::Abort);
            }
            abort_sent = true;
        }

        match tokio::time::timeout(SHELL_EVENT_POLL_INTERVAL, event_rx.recv()).await {
            Ok(Some(ShellEvent::Output(delta))) => {
                match delta.stream {
                    ToolOutputStream::Stdout => stdout.push_str(&delta.text),
                    ToolOutputStream::Stderr => stderr.push_str(&delta.text),
                }
                output(delta);
            }
            Ok(Some(ShellEvent::StreamClosed(stream))) => match stream {
                ToolOutputStream::Stdout => stdout_closed = true,
                ToolOutputStream::Stderr => stderr_closed = true,
            },
            Ok(Some(ShellEvent::Finished(result))) => {
                finished = Some(result);
            }
            Ok(Some(ShellEvent::ShellReady(control_tx))) => {
                control = Some(control_tx);
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    stdout_handle.join().map_err(|_| CoreError::new("stdout capture thread panicked"))?;
    stderr_handle.join().map_err(|_| CoreError::new("stderr capture thread panicked"))?;
    shell_handle.join().map_err(|_| CoreError::new("embedded shell thread panicked"))?;

    let exit_code =
        finished.unwrap_or_else(|| Err(CoreError::new("embedded shell exited without status")))?;

    Ok(EmbeddedShellExecution { stdout, stderr, exit_code })
}

fn run_embedded_brush_in_runtime(
    command: String,
    cwd: PathBuf,
    stdout_writer: std::io::PipeWriter,
    stderr_writer: std::io::PipeWriter,
    shell_tx: tokio::sync::mpsc::UnboundedSender<ShellEvent>,
) -> Result<i32, CoreError> {
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| {
            CoreError::new(format!("failed to build embedded {EMBEDDED_SHELL_NAME} runtime: {e}"))
        })?;

    runtime.block_on(async move {
        let mut shell = brush_core::Shell::builder()
            .no_profile(true)
            .no_rc(true)
            .default_builtins(BuiltinSet::BashMode)
            .shell_name("aia-shell".to_owned())
            .build()
            .await
            .map_err(|e| {
                CoreError::new(format!("failed to initialize embedded {EMBEDDED_SHELL_NAME}: {e}"))
            })?;

        shell
            .set_working_dir(cwd)
            .map_err(|e| CoreError::new(format!("failed to set shell working directory: {e}")))?;

        let mut params = shell.default_exec_params();
        params.set_fd(OpenFiles::STDOUT_FD, stdout_writer.into());
        params.set_fd(OpenFiles::STDERR_FD, stderr_writer.into());

        let (control_tx, control_rx) = tokio::sync::oneshot::channel::<ShellControlMessage>();
        let _ = shell_tx.send(ShellEvent::ShellReady(control_tx));

        tokio::pin!(control_rx);
        let run_result = tokio::select! {
            result = shell.run_string(command, &params) => result,
            message = &mut control_rx => {
                if matches!(message, Ok(ShellControlMessage::Abort)) {
                    for job in &shell.jobs.jobs {
                        let _ = job.kill(TrapSignal::try_from("TERM").map_err(|e| {
                            CoreError::new(format!("failed to resolve TERM signal: {e}"))
                        })?);
                    }
                    return Ok(130);
                }
                return Ok(130);
            }
        };

        run_result.map_err(|e| {
            CoreError::new(format!("embedded {EMBEDDED_SHELL_NAME} execution failed: {e}"))
        })?;

        drop(params);

        Ok(i32::from(shell.last_result()))
    })
}

fn spawn_pipe_reader(
    mut reader: std::io::PipeReader,
    stream: ToolOutputStream,
    sender: tokio::sync::mpsc::UnboundedSender<ShellEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    let text = String::from_utf8_lossy(&buffer[..size]).into_owned();
                    if sender
                        .send(ShellEvent::Output(ToolOutputDelta { stream: stream.clone(), text }))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(_) => break,
            }
        }

        let _ = sender.send(ShellEvent::StreamClosed(stream));
    })
}

#[cfg(test)]
mod tests {
    use std::{future::Future, path::Path};

    use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext, ToolOutputStream};

    use super::{ShellTool, run_embedded_brush};

    fn run_async<T>(future: impl Future<Output = T>) -> T {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build");
        runtime.block_on(future)
    }

    #[test]
    fn embedded_brush_runtime_executes_shell_command() {
        let mut deltas = Vec::new();
        let execution = run_async(run_embedded_brush(
            "printf 'ok'",
            Path::new("."),
            &AbortSignal::new(),
            &mut |delta| {
                deltas.push(delta);
            },
        ))
        .expect("embedded brush execution should succeed");

        assert_eq!(execution.stdout, "ok");
        assert_eq!(execution.stderr, "");
        assert_eq!(execution.exit_code, 0);

        let stdout = deltas
            .iter()
            .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
            .map(|delta| delta.text.as_str())
            .collect::<String>();
        let stderr = deltas
            .iter()
            .filter(|delta| matches!(delta.stream, ToolOutputStream::Stderr))
            .map(|delta| delta.text.as_str())
            .collect::<String>();

        assert_eq!(stdout, "ok");
        assert_eq!(stderr, "");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shell_call_keeps_stdout_stderr_and_exit_code_in_details() {
        let tool = ShellTool;
        let call = ToolCall::new("shell")
            .with_argument("command", "printf 'out'; printf 'err' >&2; exit 7");
        let context = ToolExecutionContext {
            run_id: "test-run".into(),
            workspace_root: Some(Path::new(".").to_path_buf()),
            abort: AbortSignal::new(),
            runtime: None,
        };
        let mut deltas = Vec::new();

        let result = tool
            .call(&call, &mut |delta| deltas.push(delta), &context)
            .await
            .expect("shell tool should return a result");

        let details = result.details.expect("shell result should include details");
        assert_eq!(details["stdout"], "out");
        assert_eq!(details["stderr"], "err");
        assert_eq!(details["exit_code"], 7);

        let stdout = deltas
            .iter()
            .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
            .map(|delta| delta.text.as_str())
            .collect::<String>();
        let stderr = deltas
            .iter()
            .filter(|delta| matches!(delta.stream, ToolOutputStream::Stderr))
            .map(|delta| delta.text.as_str())
            .collect::<String>();

        assert_eq!(stdout, "out");
        assert_eq!(stderr, "err");
    }

    #[test]
    fn embedded_brush_runtime_honors_abort_signal() {
        let abort = AbortSignal::new();
        let cancel = abort.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            cancel.abort();
        });

        let mut deltas = Vec::new();
        let execution = run_async(run_embedded_brush(
            "sleep 5; printf 'done'",
            Path::new("."),
            &abort,
            &mut |delta| deltas.push(delta),
        ))
        .expect("embedded brush should return after abort");

        assert_eq!(execution.exit_code, 130);
        let stdout = deltas
            .iter()
            .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
            .map(|delta| delta.text.as_str())
            .collect::<String>();
        assert!(!stdout.contains("done"));
    }
}
