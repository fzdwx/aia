use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::{fs::OpenOptions, io::SeekFrom};

use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolOutputStream, ToolResult,
};
use async_trait::async_trait;
use brush_builtins::{BuiltinSet, ShellBuilderExt};
use brush_core::openfiles::OpenFiles;
use brush_core::traps::TrapSignal;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const EMBEDDED_SHELL_NAME: &str = "brush";
const SHELL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);
static NEXT_CAPTURE_FILE_ID: AtomicU64 = AtomicU64::new(1);

pub struct ShellTool;

#[async_trait]
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
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
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

struct OutputCapture {
    path: PathBuf,
    writer: std::fs::File,
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

struct SignalOnDrop(Option<tokio::sync::watch::Sender<bool>>);

impl SignalOnDrop {
    fn new(sender: tokio::sync::watch::Sender<bool>) -> Self {
        Self(Some(sender))
    }
}

impl Drop for SignalOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(true);
        }
    }
}

async fn run_embedded_brush(
    command: &str,
    cwd: &Path,
    abort: &agent_core::AbortSignal,
    output: &mut (dyn FnMut(ToolOutputDelta) + Send),
) -> Result<EmbeddedShellExecution, CoreError> {
    let stdout_capture = create_output_capture(ToolOutputStream::Stdout)?;
    let stderr_capture = create_output_capture(ToolOutputStream::Stderr)?;
    let stdout_path = stdout_capture.path.clone();
    let stderr_path = stderr_capture.path.clone();
    let (capture_done_tx, capture_done_rx) = tokio::sync::watch::channel(false);

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let stdout_handle = spawn_capture_reader(
        stdout_capture.path.clone(),
        ToolOutputStream::Stdout,
        event_tx.clone(),
        capture_done_rx.clone(),
    );
    let stderr_handle = spawn_capture_reader(
        stderr_capture.path.clone(),
        ToolOutputStream::Stderr,
        event_tx.clone(),
        capture_done_rx,
    );

    let shell_command = command.to_owned();
    let shell_cwd = cwd.to_path_buf();
    let shell_tx = event_tx.clone();
    let shell_handle = tokio::spawn(async move {
        let _capture_done_signal = SignalOnDrop::new(capture_done_tx);
        let result = run_embedded_brush_in_task(
            shell_command,
            shell_cwd,
            stdout_capture.writer,
            stderr_capture.writer,
            shell_tx.clone(),
        )
        .await;
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

    stdout_handle.await.map_err(|_| CoreError::new("stdout capture task panicked"))?;
    stderr_handle.await.map_err(|_| CoreError::new("stderr capture task panicked"))?;
    shell_handle.await.map_err(|_| CoreError::new("embedded shell task panicked"))?;
    cleanup_capture_file(&stdout_path);
    cleanup_capture_file(&stderr_path);

    let exit_code =
        finished.unwrap_or_else(|| Err(CoreError::new("embedded shell exited without status")))?;

    Ok(EmbeddedShellExecution { stdout, stderr, exit_code })
}

async fn run_embedded_brush_in_task(
    command: String,
    cwd: PathBuf,
    stdout_writer: std::fs::File,
    stderr_writer: std::fs::File,
    shell_tx: tokio::sync::mpsc::UnboundedSender<ShellEvent>,
) -> Result<i32, CoreError> {
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
}

fn spawn_capture_reader(
    path: PathBuf,
    stream: ToolOutputStream,
    sender: tokio::sync::mpsc::UnboundedSender<ShellEvent>,
    mut done: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut offset = 0_u64;

        loop {
            let mut read_any = false;
            match tokio::fs::File::open(&path).await {
                Ok(mut reader) => {
                    if reader.seek(SeekFrom::Start(offset)).await.is_err() {
                        break;
                    }

                    let mut buffer = Vec::new();
                    match reader.read_to_end(&mut buffer).await {
                        Ok(size) if size > 0 => {
                            let delta_len = match u64::try_from(size) {
                                Ok(size) => size,
                                Err(_) => break,
                            };
                            offset = offset.saturating_add(delta_len);
                            read_any = true;

                            let text = String::from_utf8_lossy(&buffer).into_owned();
                            if sender
                                .send(ShellEvent::Output(ToolOutputDelta {
                                    stream: stream.clone(),
                                    text,
                                }))
                                .is_err()
                            {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if *done.borrow() {
                        break;
                    }
                }
                Err(_) => break,
            }

            if *done.borrow() && !read_any {
                break;
            }

            tokio::select! {
                result = done.changed() => {
                    if result.is_err() && *done.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(SHELL_EVENT_POLL_INTERVAL) => {}
            }
        }

        let _ = sender.send(ShellEvent::StreamClosed(stream));
    })
}

fn create_output_capture(stream: ToolOutputStream) -> Result<OutputCapture, CoreError> {
    let stream_name = stream_label(&stream);
    for _ in 0..32 {
        let path = create_capture_file_path(stream_name);
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(writer) => {
                return Ok(OutputCapture { path, writer });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CoreError::new(format!(
                    "failed to create {stream_name} capture file {}: {error}",
                    path.display()
                )));
            }
        }
    }

    Err(CoreError::new(format!("failed to allocate unique {stream_name} capture file")))
}

fn create_capture_file_path(stream_name: &str) -> PathBuf {
    let file_id = NEXT_CAPTURE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!("aia-shell-{stream_name}-{}-{file_id}.log", std::process::id()))
}

fn stream_label(stream: &ToolOutputStream) -> &'static str {
    match stream {
        ToolOutputStream::Stdout => "stdout",
        ToolOutputStream::Stderr => "stderr",
    }
}

fn cleanup_capture_file(path: &Path) {
    let _ = std::fs::remove_file(path);
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
