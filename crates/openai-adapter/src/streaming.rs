use std::{
    io::{self, BufRead},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use agent_core::{AbortSignal, StreamEvent};

use crate::OpenAiAdapterError;

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) enum StreamingSourceEvent {
    Line(String),
    Finished(Result<(), OpenAiAdapterError>),
}

pub(crate) fn stream_lines_with_abort<R, H>(
    reader: R,
    abort: &AbortSignal,
    sink: &mut dyn FnMut(StreamEvent),
    mut handle_line: H,
) -> Result<(), OpenAiAdapterError>
where
    R: io::Read + Send + 'static,
    H: FnMut(&str, &mut dyn FnMut(StreamEvent)) -> Result<bool, OpenAiAdapterError>,
{
    let (event_tx, event_rx) = mpsc::channel::<StreamingSourceEvent>();
    let reader_thread = thread::spawn(move || {
        let mut reader = io::BufReader::new(reader);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = event_tx.send(StreamingSourceEvent::Finished(Ok(())));
                    break;
                }
                Ok(_) => {
                    while line.ends_with(['\n', '\r']) {
                        line.pop();
                    }
                    if event_tx.send(StreamingSourceEvent::Line(line)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = event_tx.send(StreamingSourceEvent::Finished(Err(
                        OpenAiAdapterError::new(error.to_string()),
                    )));
                    break;
                }
            }
        }
    });

    let mut reader_finished = false;
    let result = loop {
        if abort.is_aborted() {
            break Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
        }

        match event_rx.recv_timeout(STREAM_POLL_INTERVAL) {
            Ok(StreamingSourceEvent::Line(line)) => {
                if handle_line(&line, sink)? {
                    break Ok(());
                }
            }
            Ok(StreamingSourceEvent::Finished(result)) => {
                reader_finished = true;
                break result;
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                reader_finished = true;
                break Ok(());
            }
        }
    };

    drop(event_rx);
    let _ = reader_thread.join();

    if !reader_finished && abort.is_aborted() {
        return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
    }

    result
}
