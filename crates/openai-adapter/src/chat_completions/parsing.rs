use agent_core::{Completion, CompletionSegment, ToolCall};

use crate::{OpenAiAdapterError, parse_tool_arguments};

use super::{OpenAiChatCompletionsModel, payloads::ChatCompletionsResponse};

impl OpenAiChatCompletionsModel {
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
}
