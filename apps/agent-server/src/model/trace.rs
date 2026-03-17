use agent_core::{Completion, CompletionRequest, CompletionStopReason, StreamEvent};
use agent_store::{LlmTraceEvent, LlmTraceRecord};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct TraceEventCollector {
    started_at_ms: u64,
    first_reasoning_at_ms: Option<u64>,
    first_text_at_ms: Option<u64>,
    events: Vec<LlmTraceEvent>,
}

impl TraceEventCollector {
    pub(super) fn new(started_at_ms: u64) -> Self {
        Self {
            started_at_ms,
            first_reasoning_at_ms: None,
            first_text_at_ms: None,
            events: vec![LlmTraceEvent {
                name: "request.started".to_string(),
                at_ms: started_at_ms,
                attributes: Value::Null,
            }],
        }
    }

    pub(super) fn observe(&mut self, event: &StreamEvent) {
        let at_ms = now_timestamp_ms();
        match event {
            StreamEvent::ThinkingDelta { text } => {
                if self.first_reasoning_at_ms.is_none() && !text.is_empty() {
                    self.first_reasoning_at_ms = Some(at_ms);
                    self.events.push(LlmTraceEvent {
                        name: "response.first_reasoning_delta".to_string(),
                        at_ms,
                        attributes: json!({ "preview": preview_text(text) }),
                    });
                }
            }
            StreamEvent::TextDelta { text } => {
                if self.first_text_at_ms.is_none() && !text.is_empty() {
                    self.first_text_at_ms = Some(at_ms);
                    self.events.push(LlmTraceEvent {
                        name: "response.first_text_delta".to_string(),
                        at_ms,
                        attributes: json!({ "preview": preview_text(text) }),
                    });
                }
            }
            StreamEvent::ToolCallDetected { invocation_id, tool_name, arguments } => {
                self.events.push(LlmTraceEvent {
                    name: "response.tool_call_detected".to_string(),
                    at_ms,
                    attributes: json!({
                        "invocation_id": invocation_id,
                        "tool_name": tool_name,
                        "arguments": arguments,
                    }),
                });
            }
            StreamEvent::Done => {
                self.events.push(LlmTraceEvent {
                    name: "response.stream_done".to_string(),
                    at_ms,
                    attributes: Value::Null,
                });
            }
            StreamEvent::ToolCallStarted { .. }
            | StreamEvent::ToolOutputDelta { .. }
            | StreamEvent::ToolCallCompleted { .. }
            | StreamEvent::Log { .. } => {}
        }
    }

    pub(super) fn finish_success(&mut self, record: &mut LlmTraceRecord, completion: &Completion) {
        let finished_at_ms = record.finished_at_ms.unwrap_or(record.started_at_ms);
        self.events.push(LlmTraceEvent {
            name: "response.completed".to_string(),
            at_ms: finished_at_ms,
            attributes: json!({
                "stop_reason": record.stop_reason,
                "http_status_code": record.status_code,
                "input_tokens": record.input_tokens,
                "output_tokens": record.output_tokens,
                "total_tokens": record.total_tokens,
                "cached_tokens": record.cached_tokens,
                "assistant_preview": preview_text(completion.plain_text().as_str()),
            }),
        });
        self.apply_relative_attributes(record);
        record.events = self.events.clone();
    }

    pub(super) fn finish_error(&mut self, record: &mut LlmTraceRecord) {
        let finished_at_ms = record.finished_at_ms.unwrap_or(record.started_at_ms);
        self.events.push(LlmTraceEvent {
            name: "response.failed".to_string(),
            at_ms: finished_at_ms,
            attributes: json!({
                "http_status_code": record.status_code,
                "error": record.error,
            }),
        });
        self.apply_relative_attributes(record);
        record.events = self.events.clone();
    }

    fn apply_relative_attributes(&self, record: &mut LlmTraceRecord) {
        let Some(attributes) = record.otel_attributes.as_object_mut() else {
            return;
        };

        if let Some(first_reasoning_at_ms) = self.first_reasoning_at_ms {
            attributes.insert(
                aia_config::TRACE_ATTR_FIRST_REASONING_DELTA_MS.into(),
                json!(first_reasoning_at_ms.saturating_sub(self.started_at_ms)),
            );
        }

        if let Some(first_text_at_ms) = self.first_text_at_ms {
            attributes.insert(
                aia_config::TRACE_ATTR_FIRST_TEXT_DELTA_MS.into(),
                json!(first_text_at_ms.saturating_sub(self.started_at_ms)),
            );
        }
    }
}

pub(super) fn request_summary(request: &CompletionRequest) -> Value {
    json!({
        "has_instructions": request.instructions.as_ref().is_some_and(|value| !value.is_empty()),
        "conversation_items": request.conversation.len(),
        "user_message": latest_user_message(request),
        "tool_names": request.available_tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>(),
        "max_output_tokens": request.max_output_tokens,
        "user_agent": request.user_agent,
        "prompt_cache": request.prompt_cache.as_ref().map(|cache| json!({
            "key": cache.key,
            "retention": cache.retention.as_ref().map(|value| value.as_api_value()),
        })),
    })
}

fn latest_user_message(request: &CompletionRequest) -> Option<String> {
    request
        .conversation
        .iter()
        .rev()
        .filter_map(|item| item.as_message())
        .find(|message| matches!(message.role, agent_core::Role::User))
        .map(|message| preview_text(&message.content))
}

pub(super) fn response_summary(completion: &Completion) -> Value {
    json!({
        "assistant_text": completion.plain_text(),
        "thinking_text": completion.thinking_text(),
        "stop_reason": stop_reason_label(&completion.stop_reason),
        "usage": completion.usage.as_ref().map(|usage| json!({
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens,
            "cached_tokens": usage.cached_tokens,
        })),
    })
}

pub(super) fn trace_attributes(
    request: &CompletionRequest,
    context: &agent_core::LlmTraceRequestContext,
    provider: &str,
    base_url: &str,
    endpoint_path: &str,
    streaming: bool,
) -> Value {
    json!({
        "gen_ai.operation.name": context.operation_name.as_str(),
        "gen_ai.request.model": request.model.name.as_str(),
        "gen_ai.provider.name": provider,
        "server.address": base_url,
        "http.request.method": "POST",
        "http.request.header.user_agent": request.user_agent.as_deref(),
        "http.route": endpoint_path,
        "aia.turn_id": context.turn_id.as_str(),
        "aia.run_id": context.run_id.as_str(),
        "aia.request_kind": context.request_kind.as_str(),
        "aia.step_index": context.step_index,
        "aia.streaming": streaming,
        "aia.tool_count": request.available_tools.len(),
        "gen_ai.request.max_output_tokens": request.max_output_tokens,
        "aia.prompt_cache_key": request.prompt_cache.as_ref().and_then(|cache| cache.key.as_ref()),
        "aia.prompt_cache_retention": request
            .prompt_cache
            .as_ref()
            .and_then(|cache| cache.retention.as_ref().map(|value| value.as_api_value())),
    })
}

pub(super) fn update_trace_attributes_from_completion(
    record: &mut LlmTraceRecord,
    completion: &Completion,
) {
    let Some(attributes) = record.otel_attributes.as_object_mut() else {
        return;
    };

    attributes.insert("gen_ai.response.stop_reason".into(), json!(record.stop_reason));
    attributes.insert("http.response.status_code".into(), json!(record.status_code));

    if let Some(usage) = completion.usage.as_ref() {
        attributes.insert("gen_ai.usage.input_tokens".into(), json!(usage.input_tokens));
        attributes.insert("gen_ai.usage.output_tokens".into(), json!(usage.output_tokens));
        attributes.insert("gen_ai.usage.total_tokens".into(), json!(usage.total_tokens));
        attributes.insert("gen_ai.usage.cached_tokens".into(), json!(usage.cached_tokens));
    }
}

pub(super) fn update_trace_attributes_from_error(record: &mut LlmTraceRecord) {
    let Some(attributes) = record.otel_attributes.as_object_mut() else {
        return;
    };

    attributes.insert("http.response.status_code".into(), json!(record.status_code));
    attributes.insert("error.type".into(), json!("provider_error"));
    attributes.insert("error.message".into(), json!(record.error));
}

pub(super) fn stop_reason_label(reason: &CompletionStopReason) -> String {
    match reason {
        CompletionStopReason::Stop => "stop".to_string(),
        CompletionStopReason::ToolUse => "tool_use".to_string(),
        CompletionStopReason::MaxTokens => "max_tokens".to_string(),
        CompletionStopReason::ContentFilter => "content_filter".to_string(),
        CompletionStopReason::Unknown(value) => value.clone(),
    }
}

pub(super) fn parse_status_code(error: &str) -> Option<u16> {
    error
        .split(" -> ")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|value| value.parse::<u16>().ok())
}

pub(super) fn now_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

fn preview_text(value: &str) -> String {
    let mut preview = value.chars().take(120).collect::<String>();
    if value.chars().count() > 120 {
        preview.push_str("...");
    }
    preview
}
