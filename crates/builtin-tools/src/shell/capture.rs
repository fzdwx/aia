use std::io::Read;
use std::time::Duration;

use agent_core::{CoreError, ToolOutputDelta, ToolOutputStream};

pub(super) const SHELL_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) struct OutputCapture {
    pub(super) reader: std::io::PipeReader,
    pub(super) writer: std::io::PipeWriter,
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

pub(super) fn spawn_capture_reader(
    reader: std::io::PipeReader,
    stream: ToolOutputStream,
    sender: tokio::sync::mpsc::UnboundedSender<ShellEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
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
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }

        let _ = sender.send(ShellEvent::StreamClosed(stream));
    })
}

pub(super) fn create_output_capture(stream: ToolOutputStream) -> Result<OutputCapture, CoreError> {
    std::io::pipe().map(|(reader, writer)| OutputCapture { reader, writer }).map_err(|error| {
        CoreError::new(format!("failed to create {} output pipe: {error}", stream_label(&stream)))
    })
}

fn stream_label(stream: &ToolOutputStream) -> &'static str {
    match stream {
        ToolOutputStream::Stdout => "stdout",
        ToolOutputStream::Stderr => "stderr",
    }
}
