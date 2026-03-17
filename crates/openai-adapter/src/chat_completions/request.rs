use agent_core::{CompletionRequest, CompletionStopReason, CompletionUsage};
use serde_json::{Value, json};

use crate::{
    chat_completion_messages,
    http::{apply_prompt_cache, endpoint_url},
};

use super::{OpenAiChatCompletionsModel, payloads::ChatCompletionsUsage};

impl OpenAiChatCompletionsModel {
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
        endpoint_url(&self.config.base_url, "chat/completions")
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
        apply_prompt_cache(&mut body, request.prompt_cache.as_ref());
        body
    }

    pub fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body["stream_options"] = json!({"include_usage": true});
        body
    }
}
