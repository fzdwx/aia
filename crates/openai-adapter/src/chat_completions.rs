use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};

use agent_core::{
    Completion, CompletionRequest, CompletionSegment, CompletionStopReason, CompletionUsage,
    LanguageModel, StreamEvent, ToolCall,
};
use reqwest::{
    blocking::Client,
    header::{HeaderValue, USER_AGENT},
};
use serde_json::{Value, json};

use crate::{
    ChatCompletionsResponse, ChatCompletionsUsage, OpenAiAdapterError, chat_completion_messages,
    parse_tool_arguments,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiChatCompletionsConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiChatCompletionsConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self { base_url: base_url.into(), api_key: api_key.into(), model: model.into() }
    }
}

pub struct OpenAiChatCompletionsModel {
    config: OpenAiChatCompletionsConfig,
}

#[derive(Default)]
struct StreamingToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl OpenAiChatCompletionsModel {
    fn map_usage(usage: Option<ChatCompletionsUsage>) -> Option<CompletionUsage> {
        usage.map(|usage| CompletionUsage {
            input_tokens: usage.prompt_tokens.unwrap_or(0),
            output_tokens: usage.completion_tokens.unwrap_or(0),
            total_tokens: usage.total_tokens.unwrap_or(0),
            cached_tokens: usage
                .prompt_tokens_details
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0),
        })
    }

    fn endpoint_url(&self) -> String {
        format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'))
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
        request: reqwest::blocking::RequestBuilder,
        user_agent: Option<&str>,
    ) -> reqwest::blocking::RequestBuilder {
        let Some(user_agent) = user_agent.filter(|value| !value.is_empty()) else {
            return request;
        };
        let Ok(value) = HeaderValue::from_str(user_agent) else {
            return request;
        };
        request.header(USER_AGENT, value)
    }

    fn map_finish_reason(
        finish_reason: Option<&str>,
        has_tool_calls: bool,
    ) -> CompletionStopReason {
        if has_tool_calls {
            return CompletionStopReason::ToolUse;
        }

        match finish_reason {
            Some("tool_calls") => CompletionStopReason::ToolUse,
            Some("length") => CompletionStopReason::MaxTokens,
            Some("content_filter") => CompletionStopReason::ContentFilter,
            Some("stop") | None => CompletionStopReason::Stop,
            Some(other) => CompletionStopReason::Unknown(other.to_string()),
        }
    }

    pub fn new(config: OpenAiChatCompletionsConfig) -> Result<Self, OpenAiAdapterError> {
        if config.base_url.is_empty() || config.api_key.is_empty() || config.model.is_empty() {
            return Err(OpenAiAdapterError::new("配置缺失"));
        }
        Ok(Self { config })
    }

    pub fn build_request_body(&self, request: &CompletionRequest) -> Value {
        let mut messages = Vec::new();
        if let Some(instructions) = request.instructions.as_ref().filter(|value| !value.is_empty())
        {
            messages.push(json!({
                "role": "system",
                "content": instructions,
            }));
        }
        messages.extend(chat_completion_messages(&request.conversation));

        let tools = request
            .available_tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect::<Vec<_>>();

        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
            "tools": tools,
        });
        if let Some(output_limit) = request.max_output_tokens {
            body["max_completion_tokens"] = json!(output_limit);
        }
        if let Some(prompt_cache) = request.prompt_cache.as_ref() {
            if let Some(key) = prompt_cache.key.as_ref().filter(|value| !value.is_empty()) {
                body["prompt_cache_key"] = json!(key);
            }
            if let Some(retention) = prompt_cache.retention.as_ref() {
                body["prompt_cache_retention"] = json!(retention.as_api_value());
            }
        }
        body
    }

    pub fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body["stream_options"] = json!({"include_usage": true});
        body
    }

    pub(crate) fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
        let payload: ChatCompletionsResponse = serde_json::from_str(body)
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
        let usage = Self::map_usage(payload.usage.clone());

        let mut segments = Vec::new();
        let mut finish_reason = None;
        let mut has_tool_calls = false;
        for (index, choice) in payload.choices.into_iter().enumerate() {
            if finish_reason.is_none() {
                finish_reason = choice.finish_reason.clone();
            }
            if let Some(reasoning) = choice.message.reasoning.filter(|value| !value.is_empty()) {
                segments.push(CompletionSegment::Thinking(reasoning));
            }
            if let Some(content) = choice.message.content.filter(|value| !value.is_empty()) {
                segments.push(CompletionSegment::Text(content));
            }
            for tool_call in choice.message.tool_calls {
                has_tool_calls = true;
                let invocation_id =
                    tool_call.id.unwrap_or_else(|| format!("openai-chat-call-{}", index + 1));
                segments.push(CompletionSegment::ToolUse(
                    ToolCall::new(tool_call.function.name)
                        .with_invocation_id(invocation_id)
                        .with_arguments_value(parse_tool_arguments(&tool_call.function.arguments)?),
                ));
            }
        }

        Ok(Completion {
            segments,
            stop_reason: Self::map_finish_reason(finish_reason.as_deref(), has_tool_calls),
            usage,
            response_body: Some(body.to_string()),
            http_status_code: None,
        })
    }

    pub fn config(&self) -> &OpenAiChatCompletionsConfig {
        &self.config
    }
}

impl LanguageModel for OpenAiChatCompletionsModel {
    type Error = OpenAiAdapterError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let response = self
            .apply_user_agent(
                Client::new()
                    .post(self.endpoint_url())
                    .bearer_auth(&self.config.api_key)
                    .json(&self.build_request_body(&request)),
                request.user_agent.as_deref(),
            )
            .send()
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        let body = response.text().map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        if !status.is_success() {
            return Err(self.request_failure(status, &body));
        }

        let mut completion = self.parse_response_body(&body)?;
        completion.http_status_code = Some(status.as_u16());
        Ok(completion)
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let response = self
            .apply_user_agent(
                Client::new()
                    .post(self.endpoint_url())
                    .bearer_auth(&self.config.api_key)
                    .json(&self.build_streaming_request_body(&request)),
                request.user_agent.as_deref(),
            )
            .send()
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body =
                response.text().map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
            return Err(self.request_failure(status, &body));
        }

        let reader = io::BufReader::new(response);
        let mut text_buf = String::new();
        let mut thinking_buf = String::new();
        let mut tool_calls: BTreeMap<usize, StreamingToolCallState> = BTreeMap::new();
        let mut finish_reason = None;
        let mut usage: Option<CompletionUsage> = None;
        let mut response_events = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            response_events.push(line.clone());
            if data == "[DONE]" {
                break;
            }

            let event: Value = match serde_json::from_str(data) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if usage.is_none() {
                usage = Self::map_usage(serde_json::from_value(event["usage"].clone()).ok());
            }
            let Some(delta) = event["choices"].get(0).and_then(|choice| choice.get("delta")) else {
                continue;
            };
            if finish_reason.is_none() {
                finish_reason = event["choices"]
                    .get(0)
                    .and_then(|choice| choice.get("finish_reason"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string);
            }

            if let Some(reasoning) = delta.get("reasoning").and_then(|value| value.as_str())
                && !reasoning.is_empty()
            {
                thinking_buf.push_str(reasoning);
                sink(StreamEvent::ThinkingDelta { text: reasoning.to_string() });
            }

            if let Some(content) = delta.get("content").and_then(|value| value.as_str())
                && !content.is_empty()
            {
                text_buf.push_str(content);
                sink(StreamEvent::TextDelta { text: content.to_string() });
            }

            if let Some(tool_deltas) = delta.get("tool_calls").and_then(|value| value.as_array()) {
                for tool_delta in tool_deltas {
                    let index = tool_delta
                        .get("index")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(tool_calls.len() as u64)
                        as usize;
                    let state = tool_calls.entry(index).or_default();
                    if let Some(id) = tool_delta.get("id").and_then(|value| value.as_str()) {
                        state.id = Some(id.to_string());
                    }
                    if let Some(name) = tool_delta
                        .get("function")
                        .and_then(|value| value.get("name"))
                        .and_then(|value| value.as_str())
                    {
                        if state.name.is_none() {
                            let invocation_id = state.id.clone().unwrap_or_else(|| {
                                format!("openai-chat-stream-call-{}", index + 1)
                            });
                            let arguments =
                                parse_tool_arguments(&state.arguments).unwrap_or_default();
                            sink(StreamEvent::ToolCallDetected {
                                invocation_id,
                                tool_name: name.to_string(),
                                arguments,
                            });
                        }
                        state.name = Some(name.to_string());
                    }
                    if let Some(arguments_delta) = tool_delta
                        .get("function")
                        .and_then(|value| value.get("arguments"))
                        .and_then(|value| value.as_str())
                    {
                        state.arguments.push_str(arguments_delta);
                    }
                }
            }
        }

        let mut segments = Vec::new();
        if !thinking_buf.is_empty() {
            segments.push(CompletionSegment::Thinking(thinking_buf));
        }
        if !text_buf.is_empty() {
            segments.push(CompletionSegment::Text(text_buf));
        }
        for (index, state) in tool_calls {
            let Some(name) = state.name else {
                continue;
            };
            let invocation_id =
                state.id.unwrap_or_else(|| format!("openai-chat-stream-call-{}", index + 1));
            let arguments = parse_tool_arguments(&state.arguments).unwrap_or_default();
            segments.push(CompletionSegment::ToolUse(
                ToolCall::new(name)
                    .with_invocation_id(invocation_id)
                    .with_arguments_value(arguments),
            ));
        }
        sink(StreamEvent::Done);
        let has_tool_calls =
            segments.iter().any(|segment| matches!(segment, CompletionSegment::ToolUse(_)));
        Ok(Completion {
            segments,
            stop_reason: Self::map_finish_reason(finish_reason.as_deref(), has_tool_calls),
            usage,
            response_body: Some(response_events.join("\n")),
            http_status_code: Some(status.as_u16()),
        })
    }
}
