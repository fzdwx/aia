use std::path::{Path, PathBuf};

use agent_core::{AbortSignal, CoreError, ToolOutputDelta, ToolOutputStream};
use brush_builtins::{BuiltinSet, ShellBuilderExt};
use brush_core::openfiles::OpenFiles;
use brush_core::traps::TrapSignal;

use super::capture::{
    SHELL_EVENT_POLL_INTERVAL, ShellControlMessage, ShellEvent, create_output_capture,
    spawn_capture_reader,
};

const EMBEDDED_SHELL_NAME: &str = "brush";

#[derive(Debug)]
pub(super) struct EmbeddedShellExecution {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: i32,
}

pub(super) async fn run_embedded_brush(
    command: &str,
    cwd: &Path,
    abort: &AbortSignal,
    output: &mut (dyn FnMut(ToolOutputDelta) + Send),
) -> Result<EmbeddedShellExecution, CoreError> {
    let stdout_capture = create_output_capture(ToolOutputStream::Stdout)?;
    let stderr_capture = create_output_capture(ToolOutputStream::Stderr)?;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let stdout_handle = spawn_capture_reader(
        stdout_capture.reader,
        ToolOutputStream::Stdout,
        event_tx.clone(),
    );
    let stderr_handle = spawn_capture_reader(
        stderr_capture.reader,
        ToolOutputStream::Stderr,
        event_tx.clone(),
    );

    let shell_command = command.to_owned();
    let shell_cwd = cwd.to_path_buf();
    let shell_tx = event_tx.clone();
    let shell_handle = tokio::spawn(async move {
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

    let exit_code =
        finished.unwrap_or_else(|| Err(CoreError::new("embedded shell exited without status")))?;

    Ok(EmbeddedShellExecution { stdout, stderr, exit_code })
}

async fn run_embedded_brush_in_task(
    command: String,
    cwd: PathBuf,
    stdout_writer: std::io::PipeWriter,
    stderr_writer: std::io::PipeWriter,
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
