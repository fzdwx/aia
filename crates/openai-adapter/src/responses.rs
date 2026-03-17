use std::time::Duration;

use agent_core::{
    AbortSignal, Completion, CompletionRequest, CompletionSegment, CompletionStopReason,
    CompletionUsage, LanguageModel, StreamEvent, ToolCall,
};
use async_trait::async_trait;
use reqwest::{
    Client,
    header::{HeaderValue, USER_AGENT},
};
use serde_json::{Value, json};

use crate::{
    OpenAiAdapterError, ReasoningSummaryPart, ResponsesContent, ResponsesOutput, ResponsesResponse,
    ResponsesUsage, extract_reasoning_stream_text, extract_stream_text, parse_tool_arguments,
    responses_input_item, stream_lines_with_abort,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiResponsesConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiResponsesConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self { base_url: base_url.into(), api_key: api_key.into(), model: model.into() }
    }
}

pub struct OpenAiResponsesModel {
    config: OpenAiResponsesConfig,
}

impl OpenAiResponsesModel {
    fn map_usage(usage: Option<ResponsesUsage>) -> Option<CompletionUsage> {
        usage.map(|usage| CompletionUsage {
            input_tokens: usage.input_tokens.unwrap_or(0),
            output_tokens: usage.output_tokens.unwrap_or(0),
            total_tokens: usage.total_tokens.unwrap_or(0),
            cached_tokens: usage
                .input_tokens_details
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0),
        })
    }

    fn endpoint_url(&self) -> String {
        format!("{}/responses", self.config.base_url.trim_end_matches('/'))
    }

    fn request_failure(&self, status: reqwest::StatusCode, body: &str) -> OpenAiAdapterError {
        OpenAiAdapterError::new(format!(
            "请求失败：POST {} -> {} {}",
            self.endpoint_url(),
            status,
            body
        ))
        .with_status_code(Some(status.as_u16()))
        .with_response_body(Some(body.to_string()))
    }

    fn apply_user_agent(
        &self,
        request: reqwest::RequestBuilder,
        user_agent: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let Some(user_agent) = user_agent.filter(|value| !value.is_empty()) else {
            return request;
        };
        let Ok(value) = HeaderValue::from_str(user_agent) else {
            return request;
        };
        request.header(USER_AGENT, value)
    }

    fn http_client(&self, request: &CompletionRequest) -> Result<Client, OpenAiAdapterError> {
        let mut builder = Client::builder();
        if let Some(timeout_ms) =
            request.timeout.as_ref().and_then(|timeout| timeout.read_timeout_ms)
        {
            builder = builder.timeout(Duration::from_millis(timeout_ms));
        }
        builder.build().map_err(|error| OpenAiAdapterError::new(error.to_string()))
    }

    fn map_stop_reason(
        status: Option<&str>,
        incomplete_reason: Option<&str>,
        has_tool_calls: bool,
    ) -> CompletionStopReason {
        if has_tool_calls {
            return CompletionStopReason::ToolUse;
        }

        match (status, incomplete_reason) {
            (_, Some("max_output_tokens" | "max_tokens")) => CompletionStopReason::MaxTokens,
            (Some("incomplete"), _) => CompletionStopReason::MaxTokens,
            (Some("completed") | None, _) => CompletionStopReason::Stop,
            (Some(other), _) => CompletionStopReason::Unknown(other.to_string()),
        }
    }

    pub fn new(config: OpenAiResponsesConfig) -> Result<Self, OpenAiAdapterError> {
        if config.base_url.is_empty() || config.api_key.is_empty() || config.model.is_empty() {
            return Err(OpenAiAdapterError::new("配置缺失"));
        }
        Ok(Self { config })
    }

    pub fn build_request_body(&self, request: &CompletionRequest) -> Value {
        let input = request.conversation.iter().map(responses_input_item).collect::<Vec<_>>();

        let tools = request
            .available_tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                })
            })
            .collect::<Vec<_>>();

        let mut body = json!({
            "model": self.config.model,
            "instructions": request.instructions,
            "input": input,
            "tools": tools,
        });
        if let Some(output_limit) = request.max_output_tokens {
            body["max_output_tokens"] = json!(output_limit);
        }
        if let Some(prompt_cache) = request.prompt_cache.as_ref() {
            if let Some(key) = prompt_cache.key.as_ref().filter(|value| !value.is_empty()) {
                body["prompt_cache_key"] = json!(key);
            }
            if let Some(retention) = prompt_cache.retention.as_ref() {
                body["prompt_cache_retention"] = json!(retention.as_api_value());
            }
        }
        if let Some(effort) = &request.model.reasoning_effort {
            body["reasoning"] = json!({"effort": effort, "summary": "auto"});
        }
        body
    }

    pub fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body
    }

    pub(crate) fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
        let payload: ResponsesResponse = serde_json::from_str(body)
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
        let usage = Self::map_usage(payload.usage.clone());
        let response_id = payload.id.clone();
        let status = payload.status.clone();
        let incomplete_reason =
            payload.incomplete_details.as_ref().and_then(|details| details.reason.clone());

        let mut segments = Vec::new();
        let mut has_tool_calls = false;

        for (index, item) in payload.output.into_iter().enumerate() {
            match item {
                ResponsesOutput::Reasoning { summary } => {
                    let text: String = summary
                        .into_iter()
                        .filter_map(|part| match part {
                            ReasoningSummaryPart::SummaryText { text } => Some(text),
                            ReasoningSummaryPart::Other => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        segments.push(CompletionSegment::Thinking(text));
                    }
                }
                ResponsesOutput::Message { content } => {
                    for part in content {
                        if let ResponsesContent::OutputText { text } = part {
                            segments.push(CompletionSegment::Text(text));
                        }
                    }
                }
                ResponsesOutput::FunctionCall { id, call_id, name, arguments } => {
                    has_tool_calls = true;
                    let invocation_id =
                        id.or(call_id).unwrap_or_else(|| format!("openai-call-{}", index + 1));
                    segments.push(CompletionSegment::ToolUse({
                        let mut call = ToolCall::new(name)
                            .with_invocation_id(invocation_id)
                            .with_arguments_value(parse_tool_arguments(&arguments)?);
                        if let Some(response_id) = response_id.clone() {
                            call = call.with_response_id(response_id);
                        }
                        call
                    }));
                }
                ResponsesOutput::Other => {}
            }
        }

        Ok(Completion {
            segments,
            stop_reason: Self::map_stop_reason(
                status.as_deref(),
                incomplete_reason.as_deref(),
                has_tool_calls,
            ),
            usage,
            response_body: Some(body.to_string()),
            http_status_code: None,
        })
    }

    pub fn config(&self) -> &OpenAiResponsesConfig {
        &self.config
    }
}

#[async_trait]
impl LanguageModel for OpenAiResponsesModel {
    type Error = OpenAiAdapterError;

    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let client = self.http_client(&request)?;
        let response = self
            .apply_user_agent(
                client
                    .post(self.endpoint_url())
                    .bearer_auth(&self.config.api_key)
                    .json(&self.build_request_body(&request)),
                request.user_agent.as_deref(),
            )
            .send()
            .await
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        let body =
            response.text().await.map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        if !status.is_success() {
            return Err(self.request_failure(status, &body));
        }

        let mut completion = self.parse_response_body(&body)?;
        completion.http_status_code = Some(status.as_u16());
        Ok(completion)
    }

    async fn complete_streaming_with_abort(
        &self,
        request: CompletionRequest,
        abort: &AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let client = self.http_client(&request)?;
        let response = self
            .apply_user_agent(
                client
                    .post(self.endpoint_url())
                    .bearer_auth(&self.config.api_key)
                    .json(&self.build_streaming_request_body(&request)),
                request.user_agent.as_deref(),
            )
            .send()
            .await
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
            return Err(self.request_failure(status, &body));
        }

        let mut text_buf = String::new();
        let mut thinking_buf = String::new();
        let mut saw_text_delta = false;
        let mut saw_reasoning_delta = false;
        let mut tool_calls: Vec<(String, String, String)> = Vec::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut response_id: Option<String> = None;
        let mut response_status: Option<String> = None;
        let mut incomplete_reason: Option<String> = None;
        let mut usage: Option<CompletionUsage> = None;
        let mut response_events = Vec::new();

        stream_lines_with_abort(response, abort, sink, |line, sink| {
            let Some(data) = line.strip_prefix("data: ") else {
                return Ok(false);
            };
            response_events.push(line.to_string());
            if data == "[DONE]" {
                return Ok(true);
            }
            let event: Value = match serde_json::from_str(data) {
                Ok(value) => value,
                Err(_) => return Ok(false),
            };
            match event["type"].as_str() {
                Some("response.created") => {
                    response_id = event["response"]["id"].as_str().map(ToString::to_string);
                }
                Some("response.output_text.delta") => {
                    if let Some(delta) = extract_stream_text(&event["delta"]) {
                        saw_text_delta = true;
                        text_buf.push_str(&delta);
                        sink(StreamEvent::TextDelta { text: delta });
                    }
                }
                Some("response.output_text.done") => {
                    if !saw_text_delta && let Some(text) = extract_stream_text(&event["text"]) {
                        text_buf.push_str(&text);
                        sink(StreamEvent::TextDelta { text });
                    }
                }
                Some(
                    kind @ ("response.reasoning_summary.delta"
                    | "response.reasoning_summary.done"
                    | "response.reasoning_summary_text.delta"
                    | "response.reasoning_summary_text.done"),
                ) => {
                    if let Some(delta) = extract_reasoning_stream_text(&event) {
                        let is_done_event = kind.ends_with(".done");
                        if !is_done_event || !saw_reasoning_delta {
                            saw_reasoning_delta = saw_reasoning_delta || !is_done_event;
                            thinking_buf.push_str(&delta);
                            sink(StreamEvent::ThinkingDelta { text: delta });
                        }
                    }
                }
                Some("response.function_call_arguments.delta") => {
                    let delta = event["delta"].as_str().unwrap_or("");
                    current_tool_args.push_str(delta);
                }
                Some("response.output_item.added") => {
                    let item = &event["item"];
                    if item["type"].as_str() == Some("function_call") {
                        current_tool_id = item["id"]
                            .as_str()
                            .or_else(|| item["call_id"].as_str())
                            .unwrap_or("")
                            .to_string();
                        current_tool_name = item["name"].as_str().unwrap_or("").to_string();
                        current_tool_args.clear();
                    }
                }
                Some("response.function_call_arguments.done") => {
                    if !current_tool_name.is_empty() {
                        let id = if current_tool_id.is_empty() {
                            format!("openai-stream-call-{}", tool_calls.len() + 1)
                        } else {
                            current_tool_id.clone()
                        };
                        let arguments =
                            parse_tool_arguments(&current_tool_args).unwrap_or_default();
                        sink(StreamEvent::ToolCallDetected {
                            invocation_id: id.clone(),
                            tool_name: current_tool_name.clone(),
                            arguments,
                        });
                        tool_calls.push((id, current_tool_name.clone(), current_tool_args.clone()));
                    }
                    current_tool_id.clear();
                    current_tool_name.clear();
                    current_tool_args.clear();
                }
                Some("response.completed") => {
                    if response_id.is_none() {
                        response_id = event["response"]["id"].as_str().map(ToString::to_string);
                    }
                    if response_status.is_none() {
                        response_status =
                            event["response"]["status"].as_str().map(ToString::to_string);
                    }
                    if incomplete_reason.is_none() {
                        incomplete_reason = event["response"]["incomplete_details"]["reason"]
                            .as_str()
                            .map(ToString::to_string);
                    }
                    if usage.is_none() {
                        usage = Self::map_usage(
                            serde_json::from_value(event["response"]["usage"].clone()).ok(),
                        );
                    }
                }
                Some(other) => {
                    sink(StreamEvent::Log { text: format!("[sse] {other}") });
                }
                None => {}
            }
            Ok(false)
        })
        .await?;

        let mut segments = Vec::new();
        if !thinking_buf.is_empty() {
            segments.push(CompletionSegment::Thinking(thinking_buf));
        }
        if !text_buf.is_empty() {
            segments.push(CompletionSegment::Text(text_buf));
        }
        for (id, name, args) in tool_calls {
            let arguments = parse_tool_arguments(&args).unwrap_or_default();
            segments.push(CompletionSegment::ToolUse({
                let mut call =
                    ToolCall::new(name).with_invocation_id(id).with_arguments_value(arguments);
                if let Some(response_id) = response_id.clone() {
                    call = call.with_response_id(response_id);
                }
                call
            }));
        }
        sink(StreamEvent::Done);
        let has_tool_calls =
            segments.iter().any(|segment| matches!(segment, CompletionSegment::ToolUse(_)));
        Ok(Completion {
            segments,
            stop_reason: Self::map_stop_reason(
                response_status.as_deref(),
                incomplete_reason.as_deref(),
                has_tool_calls,
            ),
            usage,
            response_body: Some(response_events.join("\n")),
            http_status_code: Some(status.as_u16()),
        })
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        self.complete_streaming_with_abort(request, &AbortSignal::new(), sink).await
    }

    fn is_cancelled_error(error: &Self::Error) -> bool {
        error.is_cancelled()
    }
}
