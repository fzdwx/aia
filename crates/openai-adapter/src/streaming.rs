use std::time::Duration;

use agent_core::{AbortSignal, Completion, CompletionRequest, StreamEvent};
use futures_util::StreamExt;
use reqwest::Response;
use serde_json::Value;
use tokio::time::timeout;

use crate::{
    OpenAiAdapterError,
    http::{apply_user_agent, http_client, request_failure},
};

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) trait StreamingState: Default {
    fn transcript_mut(&mut self) -> &mut StreamingTranscript;

    fn handle_event(
        &mut self,
        event: &Value,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(), OpenAiAdapterError>;

    fn into_completion(self, status_code: u16) -> Completion;
}

#[derive(Default)]
pub(crate) struct StreamingTranscript {
    response_events: Vec<String>,
}

impl StreamingTranscript {
    pub(crate) fn parse_json_line(
        &mut self,
        line: &str,
    ) -> Result<ParsedSseLine, OpenAiAdapterError> {
        let Some(data) = line.strip_prefix("data: ") else {
            return Ok(ParsedSseLine::Ignore);
        };
        self.response_events.push(line.to_string());
        if data == "[DONE]" {
            return Ok(ParsedSseLine::Done);
        }

        match serde_json::from_str(data) {
            Ok(event) => Ok(ParsedSseLine::Json(event)),
            Err(_) => Ok(ParsedSseLine::Ignore),
        }
    }

    pub(crate) fn into_response_body(self) -> Option<String> {
        Some(self.response_events.join("\n"))
    }
}

pub(crate) enum ParsedSseLine {
    Ignore,
    Done,
    Json(Value),
}

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

pub(crate) async fn complete_streaming_request<S>(
    endpoint_url: &str,
    api_key: &str,
    request: &CompletionRequest,
    request_body: Value,
    abort: &AbortSignal,
    sink: &mut (dyn FnMut(StreamEvent) + Send),
) -> Result<Completion, OpenAiAdapterError>
where
    S: StreamingState,
{
    let client = http_client(request)?;
    let request_builder = apply_user_agent(
        client.post(endpoint_url).bearer_auth(api_key).json(&request_body),
        request.user_agent.as_deref(),
    );
    let response =
        request_builder.send().await.map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body =
            response.text().await.map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
        return Err(request_failure(endpoint_url, status, &body));
    }

    let mut state = S::default();
    stream_lines_with_abort(response, abort, sink, |line, sink| {
        match state.transcript_mut().parse_json_line(line)? {
            ParsedSseLine::Ignore => Ok(false),
            ParsedSseLine::Done => Ok(true),
            ParsedSseLine::Json(event) => {
                state.handle_event(&event, sink)?;
                Ok(false)
            }
        }
    })
    .await?;

    sink(StreamEvent::Done);
    Ok(state.into_completion(status.as_u16()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ParsedSseLine, StreamingTranscript};

    #[test]
    fn parse_json_line_ignores_non_data_prefix() {
        let mut transcript = StreamingTranscript::default();
        let parsed = transcript.parse_json_line("event: message").expect("parse succeeds");

        assert!(matches!(parsed, ParsedSseLine::Ignore));
        assert_eq!(transcript.into_response_body(), Some(String::new()));
    }

    #[test]
    fn parse_json_line_records_done_and_invalid_json_lines() {
        let mut transcript = StreamingTranscript::default();

        let invalid =
            transcript.parse_json_line("data: {not-json}").expect("invalid json still ignored");
        let done = transcript.parse_json_line("data: [DONE]").expect("done parses");

        assert!(matches!(invalid, ParsedSseLine::Ignore));
        assert!(matches!(done, ParsedSseLine::Done));
        assert_eq!(
            transcript.into_response_body(),
            Some("data: {not-json}\ndata: [DONE]".to_string())
        );
    }

    #[test]
    fn parse_json_line_extracts_json_event() {
        let mut transcript = StreamingTranscript::default();
        let parsed = transcript
            .parse_json_line(r#"data: {"type":"response.completed"}"#)
            .expect("json parses");

        let ParsedSseLine::Json(event) = parsed else {
            panic!("expected json event");
        };
        assert_eq!(event, json!({ "type": "response.completed" }));
    }
}
