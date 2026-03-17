use std::sync::Arc;

use agent_core::{LanguageModel, StreamEvent, ToolExecutor, ToolResult};
use serde_json::json;
use session_tape::TapeEntry;

use crate::{RuntimeEvent, ToolInvocationLifecycle, ToolInvocationOutcome};

use super::super::{
    AgentRuntime, RuntimeError,
    helpers::{build_tool_source_entry_ids, now_timestamp_ms},
    tape_tools,
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
        runtime_bridge: &Arc<tape_tools::RuntimeToolContextBridge>,
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
        let tool_result_entry_id =
            self.append_tape_entry(TapeEntry::tool_result(&result).with_run_id(context.turn_id))?;
        context.source_entry_ids.push(tool_result_entry_id);
        let tool_result_event_id = self.append_tape_entry(
            TapeEntry::event(
                if failed { "tool_result_cancelled" } else { "tool_result_recorded" },
                Some(json!({
                    "tool_name": result.tool_name.clone(),
                    "status": if failed { "cancelled" } else { "ok" },
                })),
            )
            .with_run_id(context.turn_id)
            .with_meta(
                "source_entry_ids",
                json!(build_tool_source_entry_ids(
                    context.assistant_entry_id,
                    context.tool_call_entry_id,
                    tool_result_entry_id,
                )),
            ),
        )?;
        context.source_entry_ids.push(tool_result_event_id);

        on_delta(StreamEvent::ToolCallCompleted {
            invocation_id: context.call.invocation_id.clone(),
            tool_name: context.call.tool_name.clone(),
            content: result.content.clone(),
            details: result.details.clone(),
            failed,
        });

        let outcome = if failed {
            ToolInvocationOutcome::Failed { message: RuntimeError::cancelled().to_string() }
        } else {
            ToolInvocationOutcome::Succeeded { result: result.clone() }
        };
        self.publish_event(RuntimeEvent::ToolInvocation {
            call: context.call.clone(),
            outcome: outcome.clone(),
        });
        Ok(ToolInvocationLifecycle {
            call: context.call.clone(),
            started_at_ms: context.started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            trace_context: context.tool_trace_context,
            outcome,
        })
    }

    pub(super) fn record_failed_tool_call(
        &mut self,
        context: FailedToolCallContext<'_>,
        runtime_error: RuntimeError,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let failure_message = runtime_error.to_string();
        let failed_result = ToolResult::from_call(context.lifecycle.call, failure_message.clone());
        let tool_result_entry_id = self.append_tape_entry(
            TapeEntry::tool_result(&failed_result).with_run_id(context.lifecycle.turn_id),
        )?;
        context.lifecycle.source_entry_ids.push(tool_result_entry_id);
        let failure_event_id = self.append_tape_entry(
            TapeEntry::event(
                context.event_name,
                Some(json!({
                    "message": failure_message,
                    "tool_name": context.lifecycle.call.tool_name.clone(),
                })),
            )
            .with_run_id(context.lifecycle.turn_id)
            .with_meta(
                "source_entry_ids",
                json!(build_tool_source_entry_ids(
                    context.lifecycle.assistant_entry_id,
                    context.lifecycle.tool_call_entry_id,
                    tool_result_entry_id,
                )),
            ),
        )?;
        context.lifecycle.source_entry_ids.push(failure_event_id);

        let outcome = ToolInvocationOutcome::Failed { message: runtime_error.to_string() };
        self.publish_event(RuntimeEvent::ToolInvocation {
            call: context.lifecycle.call.clone(),
            outcome: outcome.clone(),
        });
        on_delta(StreamEvent::ToolCallCompleted {
            invocation_id: context.lifecycle.call.invocation_id.clone(),
            tool_name: context.lifecycle.call.tool_name.clone(),
            content: failure_message,
            details: None,
            failed: true,
        });
        Ok(ToolInvocationLifecycle {
            call: context.lifecycle.call.clone(),
            started_at_ms: context.lifecycle.started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            trace_context: context.lifecycle.tool_trace_context,
            outcome,
        })
    }
}
