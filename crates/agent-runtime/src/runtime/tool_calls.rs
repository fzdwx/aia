use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use agent_core::{
    AbortSignal, LanguageModel, LlmTraceRequestContext, RuntimeToolContext, StreamEvent, ToolCall,
    ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolResult,
};
use serde_json::json;
use session_tape::TapeEntry;

use crate::{RuntimeEvent, ToolInvocationLifecycle, ToolInvocationOutcome};

use super::{
    AgentRuntime, RuntimeError,
    helpers::build_tool_trace_context,
    helpers::{
        PreviousToolCall, build_tool_source_entry_ids, now_timestamp_ms, tool_call_signature,
    },
    tape_tools,
};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn execute_tool_call(
        &mut self,
        turn_id: &str,
        parent_trace_context: Option<&LlmTraceRequestContext>,
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: &ToolCall,
        seen_tool_calls: &mut BTreeMap<String, PreviousToolCall>,
        source_entry_ids: &mut Vec<u64>,
        on_delta: &mut dyn FnMut(StreamEvent),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let started_at_ms = now_timestamp_ms();
        let tool_trace_context =
            parent_trace_context.map(|context| build_tool_trace_context(context, call));
        let available_tool_names = self
            .visible_tools()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();
        let call_signature = tool_call_signature(call);

        if !available_tool_names.contains(&call.tool_name) {
            let runtime_error = RuntimeError::tool_unavailable(call.tool_name.clone());
            let lifecycle = self.record_failed_tool_call(
                turn_id,
                assistant_entry_id,
                tool_call_entry_id,
                call,
                started_at_ms,
                tool_trace_context.clone(),
                source_entry_ids,
                "tool_call_rejected",
                runtime_error,
                on_delta,
            )?;
            seen_tool_calls
                .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
            return Ok(lifecycle);
        }

        if tape_tools::is_runtime_tool(&call.tool_name) {
            on_delta(StreamEvent::ToolCallStarted {
                invocation_id: call.invocation_id.clone(),
                tool_name: call.tool_name.clone(),
                arguments: call.arguments.clone(),
            });

            let runtime_tools = tape_tools::build_runtime_tool_registry();
            let runtime_bridge = tape_tools::RuntimeToolContextBridge::new(self);
            let runtime_context: Rc<dyn RuntimeToolContext> = runtime_bridge.clone();
            let result = runtime_tools
                .call(
                    call,
                    &mut |_| {},
                    &ToolExecutionContext {
                        run_id: turn_id.to_string(),
                        workspace_root: self.workspace_root.clone(),
                        abort: AbortSignal::new(),
                        runtime: Some(runtime_context),
                    },
                )
                .map_err(RuntimeError::tool)?;
            self.apply_runtime_tool_handoffs(turn_id, &runtime_bridge)?;

            let tool_result_entry_id =
                self.append_tape_entry(TapeEntry::tool_result(&result).with_run_id(turn_id))?;
            source_entry_ids.push(tool_result_entry_id);
            let tool_result_event_id = self.append_tape_entry(
                TapeEntry::event(
                    "tool_result_recorded",
                    Some(json!({"tool_name": result.tool_name.clone(), "status": "ok"})),
                )
                .with_run_id(turn_id)
                .with_meta(
                    "source_entry_ids",
                    json!(build_tool_source_entry_ids(
                        assistant_entry_id,
                        tool_call_entry_id,
                        tool_result_entry_id,
                    )),
                ),
            )?;
            source_entry_ids.push(tool_result_event_id);

            on_delta(StreamEvent::ToolCallCompleted {
                invocation_id: call.invocation_id.clone(),
                tool_name: call.tool_name.clone(),
                content: result.content.clone(),
                details: result.details.clone(),
                failed: false,
            });

            let outcome = ToolInvocationOutcome::Succeeded { result: result.clone() };
            self.publish_event(RuntimeEvent::ToolInvocation {
                call: call.clone(),
                outcome: outcome.clone(),
            });
            let lifecycle = ToolInvocationLifecycle {
                call: call.clone(),
                started_at_ms,
                finished_at_ms: now_timestamp_ms(),
                trace_context: tool_trace_context,
                outcome,
            };
            seen_tool_calls
                .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
            return Ok(lifecycle);
        }

        on_delta(StreamEvent::ToolCallStarted {
            invocation_id: call.invocation_id.clone(),
            tool_name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
        });

        match self.tools.call(
            call,
            &mut |delta: ToolOutputDelta| {
                on_delta(StreamEvent::ToolOutputDelta {
                    invocation_id: call.invocation_id.clone(),
                    stream: delta.stream,
                    text: delta.text,
                });
            },
            &ToolExecutionContext {
                run_id: turn_id.to_string(),
                workspace_root: self.workspace_root.clone(),
                abort: AbortSignal::new(),
                runtime: None,
            },
        ) {
            Ok(result) => {
                if result.invocation_id != call.invocation_id || result.tool_name != call.tool_name {
                    let runtime_error = RuntimeError::tool_result_mismatch(call, &result);
                    let lifecycle = self.record_failed_tool_call(
                        turn_id,
                        assistant_entry_id,
                        tool_call_entry_id,
                        call,
                        started_at_ms,
                        tool_trace_context.clone(),
                        source_entry_ids,
                        "tool_result_rejected",
                        runtime_error,
                        on_delta,
                    )?;
                    seen_tool_calls
                        .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
                    return Ok(lifecycle);
                }

                let tool_result_entry_id =
                    self.append_tape_entry(TapeEntry::tool_result(&result).with_run_id(turn_id))?;
                source_entry_ids.push(tool_result_entry_id);
                let tool_result_event_id = self.append_tape_entry(
                    TapeEntry::event(
                        "tool_result_recorded",
                        Some(json!({"tool_name": result.tool_name.clone(), "status": "ok"})),
                    )
                    .with_run_id(turn_id)
                    .with_meta(
                        "source_entry_ids",
                        json!(build_tool_source_entry_ids(
                            assistant_entry_id,
                            tool_call_entry_id,
                            tool_result_entry_id,
                        )),
                    ),
                )?;
                source_entry_ids.push(tool_result_event_id);

                on_delta(StreamEvent::ToolCallCompleted {
                    invocation_id: call.invocation_id.clone(),
                    tool_name: call.tool_name.clone(),
                    content: result.content.clone(),
                    details: result.details.clone(),
                    failed: false,
                });

                let outcome = ToolInvocationOutcome::Succeeded { result: result.clone() };
                self.publish_event(RuntimeEvent::ToolInvocation {
                    call: call.clone(),
                    outcome: outcome.clone(),
                });
                let lifecycle = ToolInvocationLifecycle {
                    call: call.clone(),
                    started_at_ms,
                    finished_at_ms: now_timestamp_ms(),
                    trace_context: tool_trace_context,
                    outcome,
                };
                seen_tool_calls
                    .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
                Ok(lifecycle)
            }
            Err(error) => {
                let lifecycle = self.record_failed_tool_call(
                    turn_id,
                    assistant_entry_id,
                    tool_call_entry_id,
                    call,
                    started_at_ms,
                    tool_trace_context.clone(),
                    source_entry_ids,
                    "tool_call_failed",
                    RuntimeError::tool(error),
                    on_delta,
                )?;
                seen_tool_calls
                    .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
                Ok(lifecycle)
            }
        }
    }

    fn apply_runtime_tool_handoffs(
        &mut self,
        _turn_id: &str,
        runtime_bridge: &Rc<tape_tools::RuntimeToolContextBridge>,
    ) -> Result<(), RuntimeError> {
        for (name, summary) in runtime_bridge.drain_handoffs() {
            self.record_handoff(name, json!({ "summary": summary }), "ai")?;
        }
        Ok(())
    }

    fn record_failed_tool_call(
        &mut self,
        turn_id: &str,
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: &ToolCall,
        started_at_ms: u64,
        tool_trace_context: Option<crate::ToolTraceContext>,
        source_entry_ids: &mut Vec<u64>,
        event_name: &str,
        runtime_error: RuntimeError,
        on_delta: &mut dyn FnMut(StreamEvent),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let failure_message = runtime_error.to_string();
        let failed_result = ToolResult::from_call(call, failure_message.clone());
        let tool_result_entry_id =
            self.append_tape_entry(TapeEntry::tool_result(&failed_result).with_run_id(turn_id))?;
        source_entry_ids.push(tool_result_entry_id);
        let failure_event_id = self.append_tape_entry(
            TapeEntry::event(
                event_name,
                Some(json!({"message": failure_message, "tool_name": call.tool_name.clone()})),
            )
            .with_run_id(turn_id)
            .with_meta(
                "source_entry_ids",
                json!(build_tool_source_entry_ids(
                    assistant_entry_id,
                    tool_call_entry_id,
                    tool_result_entry_id,
                )),
            ),
        )?;
        source_entry_ids.push(failure_event_id);

        let outcome = ToolInvocationOutcome::Failed { message: runtime_error.to_string() };
        self.publish_event(RuntimeEvent::ToolInvocation {
            call: call.clone(),
            outcome: outcome.clone(),
        });
        on_delta(StreamEvent::ToolCallCompleted {
            invocation_id: call.invocation_id.clone(),
            tool_name: call.tool_name.clone(),
            content: failure_message.clone(),
            details: None,
            failed: true,
        });
        Ok(ToolInvocationLifecycle {
            call: call.clone(),
            started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            trace_context: tool_trace_context,
            outcome,
        })
    }
}
