use std::time::Duration;

use agent_core::{AbortSignal, StreamEvent};
use futures_util::StreamExt;
use reqwest::Response;
use tokio::time::timeout;

use crate::OpenAiAdapterError;

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(25);

fn drain_next_line(buffer: &mut Vec<u8>) -> Result<Option<String>, OpenAiAdapterError> {
    let Some(line_end) = buffer.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };

    let mut line = buffer.drain(..=line_end).collect::<Vec<_>>();
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line.pop();
    }

    String::from_utf8(line).map(Some).map_err(|error| OpenAiAdapterError::new(error.to_string()))
}

fn drain_remaining_line(buffer: &mut Vec<u8>) -> Result<Option<String>, OpenAiAdapterError> {
    if buffer.is_empty() {
        return Ok(None);
    }

    let mut line = std::mem::take(buffer);
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line.pop();
    }

    String::from_utf8(line).map(Some).map_err(|error| OpenAiAdapterError::new(error.to_string()))
}

pub(crate) async fn stream_lines_with_abort<H>(
    response: Response,
    abort: &AbortSignal,
    sink: &mut (dyn FnMut(StreamEvent) + Send),
    mut handle_line: H,
) -> Result<(), OpenAiAdapterError>
where
    H: FnMut(&str, &mut (dyn FnMut(StreamEvent) + Send)) -> Result<bool, OpenAiAdapterError>,
{
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();

    loop {
        if abort.is_aborted() {
            return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
        }

        match timeout(STREAM_POLL_INTERVAL, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                pending.extend_from_slice(&chunk);
                while let Some(line) = drain_next_line(&mut pending)? {
                    if handle_line(&line, sink)? {
                        return Ok(());
                    }
                }
            }
            Ok(Some(Err(error))) => return Err(OpenAiAdapterError::new(error.to_string())),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    if let Some(line) = drain_remaining_line(&mut pending)? {
        let _ = handle_line(&line, sink)?;
    }

    if abort.is_aborted() {
        Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"))
    } else {
        Ok(())
    }
}
