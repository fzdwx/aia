use std::sync::Arc;

use agent_runtime::{ToolInvocationOutcome, TurnLifecycle};
use agent_store::{AiaStore, LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus};
use serde_json::json;

#[derive(Clone)]
pub(crate) struct ToolTraceRecorder {
    store: Arc<AiaStore>,
}

impl ToolTraceRecorder {
    pub(crate) fn new(store: Arc<AiaStore>) -> Self {
        Self { store }
    }

    pub(crate) async fn persist_turn_spans(&self, turn: &TurnLifecycle) {
        for invocation in &turn.tool_invocations {
            let Some(context) = invocation.trace_context.as_ref() else {
                continue;
            };

            let failed = matches!(&invocation.outcome, ToolInvocationOutcome::Failed { .. });
            let cancelled = matches!(
                &invocation.outcome,
                ToolInvocationOutcome::Failed { message } if message.contains("已取消")
            );

            let (status, error, response_summary, response_body, events, details) =
                match &invocation.outcome {
                    ToolInvocationOutcome::Succeeded { result } => (
                        LlmTraceStatus::Succeeded,
                        None,
                        json!({
                            "status": "succeeded",
                            "tool_name": result.tool_name,
                            "content_preview": preview_text(&result.content),
                        }),
                        Some(result.content.clone()),
                        vec![
                            LlmTraceEvent {
                                name: "tool.started".into(),
                                at_ms: invocation.started_at_ms,
                                attributes: json!({
                                    "invocation_id": invocation.call.invocation_id,
                                    "tool_name": invocation.call.tool_name,
                                }),
                            },
                            LlmTraceEvent {
                                name: "tool.completed".into(),
                                at_ms: invocation.finished_at_ms,
                                attributes: json!({
                                    "invocation_id": result.invocation_id,
                                    "tool_name": result.tool_name,
                                    "details": result.details,
                                }),
                            },
                        ],
                        result.details.clone(),
                    ),
                    ToolInvocationOutcome::Failed { message } => (
                        LlmTraceStatus::Failed,
                        Some(message.clone()),
                        json!({
                            "status": "failed",
                            "tool_name": invocation.call.tool_name,
                            "error": message,
                        }),
                        Some(message.clone()),
                        vec![
                            LlmTraceEvent {
                                name: "tool.started".into(),
                                at_ms: invocation.started_at_ms,
                                attributes: json!({
                                    "invocation_id": invocation.call.invocation_id,
                                    "tool_name": invocation.call.tool_name,
                                }),
                            },
                            LlmTraceEvent {
                                name: "tool.failed".into(),
                                at_ms: invocation.finished_at_ms,
                                attributes: json!({
                                    "invocation_id": invocation.call.invocation_id,
                                    "tool_name": invocation.call.tool_name,
                                    "error": message,
                                }),
                            },
                        ],
                        None,
                    ),
                };

            let record = LlmTraceRecord {
                id: context.span_id.clone(),
                trace_id: context.trace_id.clone(),
                span_id: context.span_id.clone(),
                parent_span_id: Some(context.parent_span_id.clone()),
                root_span_id: context.root_span_id.clone(),
                operation_name: context.operation_name.clone(),
                span_kind: LlmTraceSpanKind::Internal,
                turn_id: turn.turn_id.clone(),
                run_id: turn.turn_id.clone(),
                request_kind: "tool".into(),
                step_index: context.parent_step_index,
                provider: "runtime".into(),
                protocol: "tool-runtime".into(),
                model: invocation.call.tool_name.clone(),
                base_url: "local://runtime".into(),
                endpoint_path: format!("/tools/{}", invocation.call.tool_name),
                streaming: false,
                started_at_ms: invocation.started_at_ms,
                finished_at_ms: Some(invocation.finished_at_ms),
                duration_ms: Some(
                    invocation.finished_at_ms.saturating_sub(invocation.started_at_ms),
                ),
                status_code: None,
                status,
                stop_reason: None,
                error,
                request_summary: json!({
                    "tool_name": invocation.call.tool_name,
                    "invocation_id": invocation.call.invocation_id,
                    "parent_request_kind": context.parent_request_kind,
                    "parent_step_index": context.parent_step_index,
                }),
                provider_request: json!({
                    "invocation_id": invocation.call.invocation_id,
                    "tool_name": invocation.call.tool_name,
                    "arguments": invocation.call.arguments,
                }),
                response_summary,
                response_body,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cached_tokens: None,
                otel_attributes: json!({
                    "aia.operation.name": context.operation_name,
                    "aia.tool.name": invocation.call.tool_name,
                    "aia.tool.invocation_id": invocation.call.invocation_id,
                    "aia.parent.request_kind": context.parent_request_kind,
                    "aia.parent.step_index": context.parent_step_index,
                    "aia.tool.failed": failed,
                    "aia.tool.cancelled": cancelled,
                    "aia.tool.details": details,
                }),
                events,
            };

            if let Err(error) = self.store.record_async(record).await {
                eprintln!("tool trace record failed: {error}");
            }
        }
    }
}

fn preview_text(value: &str) -> String {
    let mut preview = value.chars().take(120).collect::<String>();
    if value.chars().count() > 120 {
        preview.push_str("...");
    }
    preview
}
