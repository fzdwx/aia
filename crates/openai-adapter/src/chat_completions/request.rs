use std::time::Duration;

use agent_core::{CompletionRequest, CompletionStopReason, CompletionUsage};
use reqwest::{
    Client,
    header::{HeaderValue, USER_AGENT},
};
use serde_json::{Value, json};

use crate::{ChatCompletionsUsage, OpenAiAdapterError, chat_completion_messages};

use super::OpenAiChatCompletionsModel;

impl OpenAiChatCompletionsModel {
    pub(super) fn validate_request_model(
        &self,
        request: &CompletionRequest,
    ) -> Result<(), OpenAiAdapterError> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        Ok(())
    }

    pub(super) fn map_usage(usage: Option<ChatCompletionsUsage>) -> Option<CompletionUsage> {
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

    pub(super) fn endpoint_url(&self) -> String {
        format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'))
    }

    pub(super) fn request_failure(
        &self,
        status: reqwest::StatusCode,
        body: &str,
    ) -> OpenAiAdapterError {
        OpenAiAdapterError::new(format!(
            "请求失败：POST {} -> {} {}",
            self.endpoint_url(),
            status,
            body
        ))
        .with_status_code(Some(status.as_u16()))
        .with_response_body(Some(body.to_string()))
    }

    pub(super) fn apply_user_agent(
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

    pub(super) fn http_client(
        &self,
        request: &CompletionRequest,
    ) -> Result<Client, OpenAiAdapterError> {
        let mut builder = Client::builder();
        if let Some(timeout_ms) =
            request.timeout.as_ref().and_then(|timeout| timeout.read_timeout_ms)
        {
            builder = builder.timeout(Duration::from_millis(timeout_ms));
        }
        builder.build().map_err(|error| OpenAiAdapterError::new(error.to_string()))
    }

    pub(super) fn map_finish_reason(
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
}
