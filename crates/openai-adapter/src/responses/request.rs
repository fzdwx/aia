use std::time::Duration;

use agent_core::{CompletionRequest, CompletionStopReason, CompletionUsage};
use reqwest::{
    Client,
    header::{HeaderValue, USER_AGENT},
};
use serde_json::{Value, json};

use crate::{OpenAiAdapterError, ResponsesUsage, responses_input_item};

use super::OpenAiResponsesModel;

impl OpenAiResponsesModel {
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

    pub(super) fn map_usage(usage: Option<ResponsesUsage>) -> Option<CompletionUsage> {
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

    pub(super) fn endpoint_url(&self) -> String {
        format!("{}/responses", self.config.base_url.trim_end_matches('/'))
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

    pub(super) fn map_stop_reason(
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
}
