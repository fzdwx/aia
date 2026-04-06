use std::sync::Arc;

use agent_core::{LanguageModel, StreamEvent, ToolExecutor, ToolResult};
use serde_json::json;
use session_tape::TapeEntry;

use crate::{RuntimeEvent, ToolInvocationLifecycle, ToolInvocationOutcome};

use super::super::{
    AgentRuntime, RuntimeError,
    helpers::{build_tool_source_entry_ids, now_timestamp_ms},
    runtime_tool_context_adapter,
};
use super::types::{FailedToolCallContext, ToolCallLifecycleContext};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn apply_runtime_tool_handoffs(
        &mut self,
        _turn_id: &str,
        runtime_bridge: &Arc<runtime_tool_context_adapter::RuntimeToolContextAdapter>,
    ) -> Result<(), RuntimeError> {
        for (name, summary) in runtime_bridge.drain_handoffs() {
            self.record_handoff(name, json!({ "summary": summary }), "ai")?;
        }
        Ok(())
    }

    pub(super) fn record_completed_tool_call(
        &mut self,
        context: ToolCallLifecycleContext<'_>,
        result: ToolResult,
        failed: bool,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let initial_outcome = if failed {
            ToolInvocationOutcome::Failed { message: RuntimeError::cancelled().to_string() }
        } else {
            ToolInvocationOutcome::Succeeded { result }
        };
        let outcome = self.rewrite_tool_outcome(context.turn_id, &context.call, initial_outcome)?;
        self.record_tool_invocation(context, outcome, failed, None, on_delta)
    }

    pub(super) fn record_failed_tool_call(
        &mut self,
        context: FailedToolCallContext<'_>,
        runtime_error: RuntimeError,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let outcome = self.rewrite_tool_outcome(
            context.lifecycle.turn_id,
            &context.lifecycle.call,
            ToolInvocationOutcome::Failed { message: runtime_error.to_string() },
        )?;
        self.record_tool_invocation(
            context.lifecycle,
            outcome,
            false,
            Some(context.event_name),
            on_delta,
        )
    }

    fn record_tool_invocation(
        &mut self,
        context: ToolCallLifecycleContext<'_>,
        outcome: ToolInvocationOutcome,
        failed: bool,
        failure_event_name: Option<&str>,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let (tool_result, event_name, event_body, stream_failed) = match &outcome {
            ToolInvocationOutcome::Succeeded { result } => (
                result.clone(),
                if failed { "tool_result_cancelled" } else { "tool_result_recorded" },
                json!({
                    "tool_name": result.tool_name.clone(),
                    "status": if failed { "cancelled" } else { "ok" },
                }),
                failed,
            ),
            ToolInvocationOutcome::Failed { message } => (
                ToolResult::from_call(context.call, message.clone()),
                failure_event_name.unwrap_or(if failed {
                    "tool_result_cancelled"
                } else {
                    "tool_call_failed"
                }),
                json!({
                    "message": message,
                    "tool_name": context.call.tool_name.clone(),
                }),
                true,
            ),
        };

        let tool_result_entry_id = self
            .append_tape_entry(TapeEntry::tool_result(&tool_result).with_run_id(context.turn_id))?;
        context.source_entry_ids.push(tool_result_entry_id);
        let tool_result_event_id = self.append_tape_entry(
            TapeEntry::event(event_name, Some(event_body)).with_run_id(context.turn_id).with_meta(
                "source_entry_ids",
                json!(build_tool_source_entry_ids(
                    context.assistant_entry_id,
                    context.tool_call_entry_id,
                    tool_result_entry_id,
                )),
            ),
        )?;
        context.source_entry_ids.push(tool_result_event_id);

        let finished_at_ms = now_timestamp_ms();

        on_delta(StreamEvent::ToolCallCompleted {
            invocation_id: context.call.invocation_id.clone(),
            tool_name: context.call.tool_name.clone(),
            content: tool_result.content.clone(),
            details: tool_result.details.clone(),
            failed: stream_failed,
            finished_at_ms,
        });

        self.publish_event(RuntimeEvent::ToolInvocation {
            call: context.call.clone(),
            outcome: outcome.clone(),
        });
        Ok(ToolInvocationLifecycle {
            call: context.call.clone(),
            started_at_ms: context.started_at_ms,
            finished_at_ms,
            trace_context: context.tool_trace_context,
            replay_events: Vec::new(),
            outcome,
        })
    }
}
