use std::time::Duration;

use agent_core::{AbortSignal, CoreError};
use serde::Deserialize;

pub(crate) const API_BASE_URL: &str = "https://mcp.exa.ai";
pub(crate) const API_MCP_ENDPOINT: &str = "/mcp";

#[derive(Deserialize)]
struct ExaMcpResponse {
    result: Option<ExaMcpResult>,
}

#[derive(Deserialize)]
struct ExaMcpResult {
    content: Vec<ExaMcpContent>,
}

#[derive(Deserialize)]
struct ExaMcpContent {
    text: Option<String>,
}

pub(crate) fn endpoint_url(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path.trim_start_matches('/'))
}

pub(crate) fn map_request_error(
    error: reqwest::Error,
    timeout_message: &str,
    prefix: &str,
) -> CoreError {
    if error.is_timeout() {
        return CoreError::new(timeout_message);
    }
    CoreError::new(format!("{prefix}: {error}"))
}

pub(crate) fn parse_exa_response(body: &str) -> Result<Option<String>, CoreError> {
    if let Ok(content) = parse_exa_payload(body) {
        return Ok(content);
    }

    let mut saw_data_line = false;
    let mut last_error: Option<CoreError> = None;
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("data:") {
            continue;
        }
        saw_data_line = true;
        let payload = trimmed.trim_start_matches("data:").trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        match parse_exa_payload(payload) {
            Ok(content) => {
                if content.is_some() {
                    return Ok(content);
                }
            }
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(error) = last_error.filter(|_| saw_data_line) {
        return Err(error);
    }
    Ok(None)
}

fn parse_exa_payload(payload: &str) -> Result<Option<String>, CoreError> {
    let response: ExaMcpResponse = serde_json::from_str(payload)
        .map_err(|error| CoreError::new(format!("failed to parse Exa response: {error}")))?;
    Ok(response.result.and_then(|result| {
        result.content.into_iter().find_map(|item| item.text.filter(|text| !text.trim().is_empty()))
    }))
}

pub(crate) enum RequestRace<T> {
    Completed(T),
    Cancelled,
}

pub(crate) async fn race_abort<F>(future: F, abort: AbortSignal) -> RequestRace<F::Output>
where
    F: std::future::Future,
{
    tokio::pin!(future);
    tokio::select! {
        output = &mut future => RequestRace::Completed(output),
        _ = wait_for_abort(abort) => RequestRace::Cancelled,
    }
}

async fn wait_for_abort(abort: AbortSignal) {
    while !abort.is_aborted() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
