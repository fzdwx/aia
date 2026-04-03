use std::time::Duration;

use agent_core::{AbortSignal, Completion, CompletionRequest, StreamEvent};
use futures_util::StreamExt;
use reqwest::{Client, Response};
use serde_json::Value;
use tokio::time::timeout;

use crate::{
    OpenAiAdapterError,
    http::{apply_user_agent, http_client, request_failure},
    retry::{RetryPolicy, StreamingAttemptState, backoff_delay, should_retry},
};

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(25);

async fn send_with_abort(
    request_builder: reqwest::RequestBuilder,
    abort: &AbortSignal,
) -> Result<Response, OpenAiAdapterError> {
    let send_future = request_builder.send();
    tokio::pin!(send_future);

    loop {
        if abort.is_aborted() {
            return Err(OpenAiAdapterError::cancelled("OpenAI 请求在发送阶段已取消"));
        }

        match timeout(STREAM_POLL_INTERVAL, &mut send_future).await {
            Ok(Ok(response)) => return Ok(response),
            Ok(Err(error)) => {
                return Err(OpenAiAdapterError::new(error.to_string()).with_retryable(true));
            }
            Err(_) => continue,
        }
    }
}

async fn sleep_with_abort(delay: Duration, abort: &AbortSignal) -> Result<(), OpenAiAdapterError> {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);

    loop {
        if abort.is_aborted() {
            return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
        }

        match timeout(STREAM_POLL_INTERVAL, &mut sleep).await {
            Ok(_) => return Ok(()),
            Err(_) => continue,
        }
    }
}

pub(crate) trait StreamingState: Default {
    fn transcript_mut(&mut self) -> &mut StreamingTranscript;

    fn handle_event(
        &mut self,
        event: &Value,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(), OpenAiAdapterError>;

    fn saw_terminal_event(&self) -> bool {
        false
    }

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

    pub(crate) fn response_body(&self) -> Option<String> {
        Some(self.response_events.join("\n"))
    }

    pub(crate) fn into_response_body(self) -> Option<String> {
        self.response_body()
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
) -> Result<bool, OpenAiAdapterError>
where
    H: FnMut(&str, &mut (dyn FnMut(StreamEvent) + Send)) -> Result<bool, OpenAiAdapterError>,
{
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut saw_done = false;

    loop {
        if abort.is_aborted() {
            return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
        }

        match timeout(STREAM_POLL_INTERVAL, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                pending.extend_from_slice(&chunk);
                while let Some(line) = drain_next_line(&mut pending)? {
                    if handle_line(&line, sink)? {
                        return Ok(true);
                    }
                }
            }
            Ok(Some(Err(error))) => {
                return Err(OpenAiAdapterError::new(error.to_string()).with_retryable(true));
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    if let Some(line) = drain_remaining_line(&mut pending)? {
        if handle_line(&line, sink)? {
            saw_done = true;
        }
    }

    if abort.is_aborted() {
        Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"))
    } else {
        Ok(saw_done)
    }
}

async fn run_single_streaming_attempt<S>(
    endpoint_url: &str,
    client: &Client,
    api_key: &str,
    request: &CompletionRequest,
    request_body: &Value,
    abort: &AbortSignal,
    sink: &mut (dyn FnMut(StreamEvent) + Send),
) -> Result<Completion, OpenAiAdapterError>
where
    S: StreamingState,
{
    if abort.is_aborted() {
        return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
    }

    let request_builder = apply_user_agent(
        client.post(endpoint_url).bearer_auth(api_key).json(&request_body),
        request.user_agent.as_deref(),
    );

    let response = send_with_abort(request_builder, abort).await?;

    let status = response.status();
    if !status.is_success() {
        let body =
            response.text().await.map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
        return Err(request_failure(endpoint_url, status, &body));
    }

    // Check abort after receiving response headers
    if abort.is_aborted() {
        return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
    }

    let mut state = S::default();
    let saw_done = stream_lines_with_abort(response, abort, sink, |line, sink| {
        match state.transcript_mut().parse_json_line(line)? {
            ParsedSseLine::Ignore => Ok(false),
            ParsedSseLine::Done => Ok(true),
            ParsedSseLine::Json(event) => {
                state.handle_event(&event, sink)?;
                Ok(false)
            }
        }
    })
    .await
    .map_err(|error| {
        if error.response_body().is_some() {
            error
        } else {
            error.with_response_body(state.transcript_mut().response_body())
        }
    })?;

    if !saw_done && !state.saw_terminal_event() {
        return Err(OpenAiAdapterError::new("OpenAI 流式响应在完成前提前结束")
            .with_retryable(true)
            .with_response_body(state.transcript_mut().response_body()));
    }

    sink(StreamEvent::Done);
    Ok(state.into_completion(status.as_u16()))
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
    if abort.is_aborted() {
        return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
    }

    let client = http_client(request)?;
    let policy = RetryPolicy::default();

    for attempt_index in 1..=policy.max_attempts {
        if abort.is_aborted() {
            return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
        }

        let mut attempt_state = StreamingAttemptState::default();
        let mut tracked_sink = |event: StreamEvent| {
            attempt_state.record_event(&event);
            sink(event);
        };

        let result = run_single_streaming_attempt::<S>(
            endpoint_url,
            &client,
            api_key,
            request,
            &request_body,
            abort,
            &mut tracked_sink,
        )
        .await;

        match result {
            Ok(completion) => return Ok(completion),
            Err(error) if error.is_cancelled() || abort.is_aborted() => {
                return Err(OpenAiAdapterError::cancelled("OpenAI 流式请求已取消"));
            }
            Err(error)
                if attempt_index < policy.max_attempts
                    && attempt_state.can_retry()
                    && should_retry(&error) =>
            {
                let delay = backoff_delay(policy, attempt_index);
                sink(StreamEvent::Retrying {
                    attempt: attempt_index,
                    max_attempts: policy.max_attempts,
                    reason: error.to_string(),
                });
                sleep_with_abort(delay, abort).await?;
            }
            Err(error) => return Err(error),
        }
    }

    Err(OpenAiAdapterError::new("OpenAI 流式请求重试次数已耗尽"))
}

#[cfg(test)]
#[path = "../tests/streaming/mod.rs"]
mod tests;
