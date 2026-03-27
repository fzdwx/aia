use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{Completion, CompletionSegment, CompletionUsage, StreamEvent, ToolCall};
use serde_json::Value;

use crate::{
    OpenAiAdapterError, StreamingToolCallAccumulator, extract_reasoning_stream_text,
    extract_stream_text, parse_tool_arguments,
    streaming::{StreamingState, StreamingTranscript},
};

use super::OpenAiResponsesModel;

fn now_timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn error_message(value: &Value) -> Option<String> {
    value.as_str().filter(|text| !text.trim().is_empty()).map(ToString::to_string)
}

fn response_stream_error(event: &Value) -> Option<String> {
    error_message(&event["error"]["message"])
        .or_else(|| error_message(&event["response"]["error"]["message"]))
        .or_else(|| error_message(&event["message"]))
        .or_else(|| event["response"]["status"].as_str().map(|status| format!("response {status}")))
}

fn is_top_level_error_envelope(event: &Value) -> bool {
    event.get("type").is_none() && event.get("error").is_some()
}

#[derive(Default)]
pub(super) struct ResponsesStreamingState {
    text_buf: String,
    thinking_buf: String,
    saw_text_delta: bool,
    saw_reasoning_delta: bool,
    tool_calls: Vec<(String, String, String)>,
    current_tool: StreamingToolCallAccumulator,
    response_id: Option<String>,
    response_status: Option<String>,
    incomplete_reason: Option<String>,
    usage: Option<CompletionUsage>,
    transcript: StreamingTranscript,
}

impl ResponsesStreamingState {
    fn finish_current_tool_call(&mut self, sink: &mut (dyn FnMut(StreamEvent) + Send)) {
        if self.current_tool.is_empty() {
            self.current_tool.clear();
            return;
        }

        let Some(tool_name) = self.current_tool.tool_name().map(ToString::to_string) else {
            self.current_tool.clear();
            return;
        };

        let id = self
            .current_tool
            .invocation_id_or(|| format!("openai-stream-call-{}", self.tool_calls.len() + 1));
        if !self.current_tool.detection_emitted() {
            sink(StreamEvent::ToolCallDetected {
                invocation_id: id.clone(),
                tool_name: tool_name.clone(),
                arguments: self.current_tool.parsed_arguments(),
                detected_at_ms: now_timestamp_ms(),
            });
        }
        self.tool_calls.push((id, tool_name, self.current_tool.raw_arguments().to_string()));
        self.current_tool.clear();
    }
}

impl StreamingState for ResponsesStreamingState {
    fn transcript_mut(&mut self) -> &mut StreamingTranscript {
        &mut self.transcript
    }

    fn handle_event(
        &mut self,
        event: &Value,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(), OpenAiAdapterError> {
        if is_top_level_error_envelope(event) {
            let message = response_stream_error(event)
                .unwrap_or_else(|| "OpenAI Responses stream failed".to_string());
            return Err(OpenAiAdapterError::new(message));
        }

        match event["type"].as_str() {
            Some("response.created") => {
                self.response_id = event["response"]["id"].as_str().map(ToString::to_string);
            }
            Some("response.output_text.delta") => {
                if let Some(delta) = extract_stream_text(&event["delta"]) {
                    self.saw_text_delta = true;
                    self.text_buf.push_str(&delta);
                    sink(StreamEvent::TextDelta { text: delta });
                }
            }
            Some("response.output_text.done") => {
                if !self.saw_text_delta
                    && let Some(text) = extract_stream_text(&event["text"])
                {
                    self.text_buf.push_str(&text);
                    sink(StreamEvent::TextDelta { text });
                }
            }
            Some(
                kind @ ("response.reasoning_summary.delta"
                | "response.reasoning_summary.done"
                | "response.reasoning_summary_text.delta"
                | "response.reasoning_summary_text.done"),
            ) => {
                if let Some(delta) = extract_reasoning_stream_text(event) {
                    let is_done_event = kind.ends_with(".done");
                    if !is_done_event || !self.saw_reasoning_delta {
                        self.saw_reasoning_delta = self.saw_reasoning_delta || !is_done_event;
                        self.thinking_buf.push_str(&delta);
                        sink(StreamEvent::ThinkingDelta { text: delta });
                    }
                }
            }
            Some("response.function_call_arguments.delta") => {
                let delta = event["delta"].as_str().unwrap_or("");
                self.current_tool.push_arguments_delta(delta);
            }
            Some("response.output_item.added") => {
                let item = &event["item"];
                if item["type"].as_str() == Some("function_call") {
                    self.current_tool.clear();
                    if let Some(invocation_id) =
                        item["id"].as_str().or_else(|| item["call_id"].as_str())
                    {
                        self.current_tool.set_invocation_id(invocation_id.to_string());
                    }
                    if let Some(tool_name) = item["name"].as_str().filter(|value| !value.is_empty())
                    {
                        self.current_tool.set_tool_name(tool_name.to_string());
                    }
                }
            }
            Some("response.function_call_arguments.done") => {
                self.finish_current_tool_call(sink);
            }
            Some("error" | "response.failed") => {
                let message = response_stream_error(event)
                    .unwrap_or_else(|| "OpenAI Responses stream failed".to_string());
                return Err(OpenAiAdapterError::new(message));
            }
            Some("response.completed") => {
                if self.response_id.is_none() {
                    self.response_id = event["response"]["id"].as_str().map(ToString::to_string);
                }
                if self.response_status.is_none() {
                    self.response_status =
                        event["response"]["status"].as_str().map(ToString::to_string);
                }
                if self.incomplete_reason.is_none() {
                    self.incomplete_reason = event["response"]["incomplete_details"]["reason"]
                        .as_str()
                        .map(ToString::to_string);
                }
                if self.usage.is_none() {
                    self.usage = OpenAiResponsesModel::map_usage(
                        serde_json::from_value(event["response"]["usage"].clone()).ok(),
                    );
                }
            }
            Some(other) => {
                sink(StreamEvent::Log { text: format!("[sse] {other}") });
            }
            None => {}
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
        for (id, name, args) in self.tool_calls {
            let arguments = parse_tool_arguments(&args).unwrap_or_default();
            segments.push(CompletionSegment::ToolUse({
                let mut call =
                    ToolCall::new(name).with_invocation_id(id).with_arguments_value(arguments);
                if let Some(response_id) = self.response_id.clone() {
                    call = call.with_response_id(response_id);
                }
                call
            }));
        }

        let has_tool_calls =
            segments.iter().any(|segment| matches!(segment, CompletionSegment::ToolUse(_)));

        Completion {
            segments,
            stop_reason: OpenAiResponsesModel::map_stop_reason(
                self.response_status.as_deref(),
                self.incomplete_reason.as_deref(),
                has_tool_calls,
            ),
            usage: self.usage,
            response_body: self.transcript.into_response_body(),
            http_status_code: Some(status_code),
        }
    }
}
