use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs::OpenOptions, io::SeekFrom, time::Duration};

use agent_core::{CoreError, ToolOutputDelta, ToolOutputStream};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub(super) const SHELL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

static NEXT_CAPTURE_FILE_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct OutputCapture {
    pub(super) path: PathBuf,
    pub(super) writer: std::fs::File,
}

pub(super) enum ShellEvent {
    Output(ToolOutputDelta),
    StreamClosed(ToolOutputStream),
    Finished(Result<i32, CoreError>),
    ShellReady(tokio::sync::oneshot::Sender<ShellControlMessage>),
}

#[derive(Clone, Copy)]
pub(super) enum ShellControlMessage {
    Abort,
}

pub(super) struct SignalOnDrop(Option<tokio::sync::watch::Sender<bool>>);

impl SignalOnDrop {
    pub(super) fn new(sender: tokio::sync::watch::Sender<bool>) -> Self {
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

pub(super) fn spawn_capture_reader(
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

pub(super) fn create_output_capture(stream: ToolOutputStream) -> Result<OutputCapture, CoreError> {
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

pub(super) fn cleanup_capture_file(path: &Path) {
    let _ = std::fs::remove_file(path);
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
