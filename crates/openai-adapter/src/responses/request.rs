use agent_core::{CompletionRequest, CompletionStopReason, CompletionUsage};
use serde_json::{Value, json};

use crate::{
    ResponsesUsage,
    http::{apply_prompt_cache, endpoint_url},
    responses_input_item,
};

use super::OpenAiResponsesModel;

impl OpenAiResponsesModel {
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
        endpoint_url(&self.config.base_url, "responses")
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
        apply_prompt_cache(&mut body, request.prompt_cache.as_ref());
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
