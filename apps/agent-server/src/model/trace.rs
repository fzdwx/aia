use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agent_core::{Completion, CompletionRequest, CompletionStopReason, StreamEvent};
use agent_store::{AiaStore, LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus};
use serde_json::{Value, json};

use super::{ServerModel, ServerModelError, ServerModelInner};

pub(super) struct TraceEventCollector {
    started_at_ms: u64,
    first_reasoning_at_ms: Option<u64>,
    first_text_at_ms: Option<u64>,
    events: Vec<LlmTraceEvent>,
}

pub(super) struct ModelTraceRecorder {
    store: Option<Arc<AiaStore>>,
    trace_seed: Option<LlmTraceRecord>,
    event_collector: TraceEventCollector,
    started_at_ms: u64,
}

impl ModelTraceRecorder {
    pub(super) fn new(
        model: &ServerModel,
        request: &CompletionRequest,
        started_at_ms: u64,
        streaming: bool,
    ) -> Self {
        Self {
            store: model.trace_store.clone(),
            trace_seed: build_trace_seed(model, request, streaming),
            event_collector: TraceEventCollector::new(started_at_ms),
            started_at_ms,
        }
    }

    pub(super) fn observe(&mut self, event: &StreamEvent) {
        self.event_collector.observe(event);
    }

    pub(super) fn finish(
        mut self,
        duration: Duration,
        result: &Result<Completion, ServerModelError>,
    ) {
        let Some(store) = self.store.take() else {
            return;
        };
        let Some(mut record) = self.trace_seed.take() else {
            return;
        };

        record.started_at_ms = self.started_at_ms;
        record.finished_at_ms =
            Some(self.started_at_ms.saturating_add(duration.as_millis() as u64));
        record.duration_ms = Some(duration.as_millis() as u64);

        match result {
            Ok(completion) => {
                record.status = LlmTraceStatus::Succeeded;
                record.status_code = completion.http_status_code;
                record.stop_reason = Some(stop_reason_label(&completion.stop_reason));
                record.response_summary = response_summary(completion);
                record.response_body =
                    completion.response_body.clone().or_else(|| Some(completion.plain_text()));
                record.input_tokens = completion.usage.as_ref().map(|usage| usage.input_tokens);
                record.output_tokens = completion.usage.as_ref().map(|usage| usage.output_tokens);
                record.total_tokens = completion.usage.as_ref().map(|usage| usage.total_tokens);
                record.cached_tokens = completion.usage.as_ref().map(|usage| usage.cached_tokens);
                update_trace_attributes_from_completion(&mut record, completion);
                self.event_collector.finish_success(&mut record, completion);
            }
            Err(error) => {
                record.status = LlmTraceStatus::Failed;
                record.error = Some(error.to_string());
                record.status_code =
                    error.status_code().or_else(|| parse_status_code(error.to_string().as_str()));
                record.response_body = error.response_body().map(str::to_string);
                record.response_summary = json!({
                    "error": error.to_string(),
                    "http_status_code": record.status_code,
                });
                update_trace_attributes_from_error(&mut record);
                self.event_collector.finish_error(&mut record);
            }
        }

        std::thread::spawn(move || {
            if let Err(error) = agent_store::LlmTraceStore::record(store.as_ref(), &record) {
                eprintln!("trace record failed: {error}");
            }
        });
    }
}

fn build_trace_seed(
    model: &ServerModel,
    request: &CompletionRequest,
    streaming: bool,
) -> Option<LlmTraceRecord> {
    let context = request.trace_context.as_ref()?;
    let (provider, protocol, base_url, endpoint_path, provider_request) = match &model.inner {
        ServerModelInner::Bootstrap(_) => return None,
        ServerModelInner::OpenAiResponses(model) => (
            "openai".to_string(),
            "openai-responses".to_string(),
            model.config().base_url.clone(),
            "/responses".to_string(),
            if streaming {
                model.build_streaming_request_body(request)
            } else {
                model.build_request_body(request)
            },
        ),
        ServerModelInner::OpenAiChatCompletions(model) => (
            "openai".to_string(),
            "openai-chat-completions".to_string(),
            model.config().base_url.clone(),
            "/chat/completions".to_string(),
            if streaming {
                model.build_streaming_request_body(request)
            } else {
                model.build_request_body(request)
            },
        ),
    };

    let otel_attributes = trace_attributes(
        request,
        context,
        provider.as_str(),
        base_url.as_str(),
        endpoint_path.as_str(),
        streaming,
    );

    Some(LlmTraceRecord {
        id: context.span_id.clone(),
        trace_id: context.trace_id.clone(),
        span_id: context.span_id.clone(),
        parent_span_id: context.parent_span_id.clone(),
        root_span_id: context.root_span_id.clone(),
        operation_name: context.operation_name.clone(),
        span_kind: LlmTraceSpanKind::Client,
        session_id: context.session_id.clone(),
        turn_id: context.turn_id.clone(),
        run_id: context.run_id.clone(),
        request_kind: context.request_kind.clone(),
        step_index: context.step_index,
        provider,
        protocol,
        model: request.model.name.clone(),
        base_url,
        endpoint_path,
        streaming,
        started_at_ms: 0,
        finished_at_ms: None,
        duration_ms: None,
        status_code: None,
        status: LlmTraceStatus::Succeeded,
        stop_reason: None,
        error: None,
        request_summary: request_summary(request),
        provider_request,
        response_summary: Value::Null,
        response_body: None,
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cached_tokens: None,
        otel_attributes,
        events: vec![],
    })
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
            StreamEvent::ToolCallDetected { invocation_id, tool_name, arguments, .. } => {
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
            StreamEvent::ToolCallArgumentsDelta { invocation_id, tool_name, arguments_delta } => {
                self.events.push(LlmTraceEvent {
                    name: "response.tool_call_arguments_delta".to_string(),
                    at_ms,
                    attributes: json!({
                        "invocation_id": invocation_id,
                        "tool_name": tool_name,
                        "arguments_delta_preview": preview_text(arguments_delta),
                    }),
                });
            }
            StreamEvent::ToolCallReady { call } => {
                self.events.push(LlmTraceEvent {
                    name: "response.tool_call_ready".to_string(),
                    at_ms,
                    attributes: json!({
                        "invocation_id": call.invocation_id,
                        "tool_name": call.tool_name,
                        "arguments": call.arguments,
                    }),
                });
            }
            StreamEvent::Retrying { attempt, max_attempts, reason } => {
                self.events.push(LlmTraceEvent {
                    name: "response.retrying".to_string(),
                    at_ms,
                    attributes: json!({
                        "attempt": attempt,
                        "max_attempts": max_attempts,
                        "reason": reason,
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
            StreamEvent::WidgetHostCommand { .. }
            | StreamEvent::WidgetClientEvent { .. }
            | StreamEvent::ToolCallStarted { .. }
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
