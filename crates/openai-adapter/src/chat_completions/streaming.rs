use std::collections::BTreeMap;

use agent_core::{Completion, CompletionSegment, CompletionUsage, StreamEvent, ToolCall};
use serde_json::Value;

use crate::{
    OpenAiAdapterError, parse_tool_arguments,
    streaming::{StreamingState, StreamingTranscript},
};

use super::OpenAiChatCompletionsModel;

#[derive(Default)]
struct StreamingToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Default)]
pub(super) struct ChatCompletionsStreamingState {
    text_buf: String,
    thinking_buf: String,
    tool_calls: BTreeMap<usize, StreamingToolCallState>,
    finish_reason: Option<String>,
    usage: Option<CompletionUsage>,
    transcript: StreamingTranscript,
}

impl ChatCompletionsStreamingState {
    fn apply_tool_delta(&mut self, tool_delta: &Value, sink: &mut (dyn FnMut(StreamEvent) + Send)) {
        let index = tool_delta
            .get("index")
            .and_then(|value| value.as_u64())
            .unwrap_or(self.tool_calls.len() as u64) as usize;
        let state = self.tool_calls.entry(index).or_default();
        if let Some(id) = tool_delta.get("id").and_then(|value| value.as_str()) {
            state.id = Some(id.to_string());
        }
        if let Some(name) = tool_delta
            .get("function")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str())
        {
            if state.name.is_none() {
                let invocation_id = state
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("openai-chat-stream-call-{}", index + 1));
                let arguments = parse_tool_arguments(&state.arguments).unwrap_or_default();
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

impl StreamingState for ChatCompletionsStreamingState {
    fn transcript_mut(&mut self) -> &mut StreamingTranscript {
        &mut self.transcript
    }

    fn handle_event(
        &mut self,
        event: &Value,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(), OpenAiAdapterError> {
        if self.usage.is_none() {
            self.usage = OpenAiChatCompletionsModel::map_usage(
                serde_json::from_value(event["usage"].clone()).ok(),
            );
        }
        let Some(delta) = event["choices"].get(0).and_then(|choice| choice.get("delta")) else {
            return Ok(());
        };
        if self.finish_reason.is_none() {
            self.finish_reason = event["choices"]
                .get(0)
                .and_then(|choice| choice.get("finish_reason"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
        }

        if let Some(reasoning) = delta.get("reasoning").and_then(|value| value.as_str())
            && !reasoning.is_empty()
        {
            self.thinking_buf.push_str(reasoning);
            sink(StreamEvent::ThinkingDelta { text: reasoning.to_string() });
        }

        if let Some(content) = delta.get("content").and_then(|value| value.as_str())
            && !content.is_empty()
        {
            self.text_buf.push_str(content);
            sink(StreamEvent::TextDelta { text: content.to_string() });
        }

        if let Some(tool_deltas) = delta.get("tool_calls").and_then(|value| value.as_array()) {
            for tool_delta in tool_deltas {
                self.apply_tool_delta(tool_delta, sink);
            }
        }

        Ok(())
    }

    fn into_completion(self, status_code: u16) -> Completion {
        let mut segments = Vec::new();
        if !self.thinking_buf.is_empty() {
            segments.push(CompletionSegment::Thinking(self.thinking_buf));
        }
        if !self.text_buf.is_empty() {
            segments.push(CompletionSegment::Text(self.text_buf));
        }
        for (index, state) in self.tool_calls {
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

        let has_tool_calls =
            segments.iter().any(|segment| matches!(segment, CompletionSegment::ToolUse(_)));

        Completion {
            segments,
            stop_reason: OpenAiChatCompletionsModel::map_finish_reason(
                self.finish_reason.as_deref(),
                has_tool_calls,
            ),
            usage: self.usage,
            response_body: self.transcript.into_response_body(),
            http_status_code: Some(status_code),
        }
    }
}
