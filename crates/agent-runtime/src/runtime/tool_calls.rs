use std::collections::{BTreeMap, BTreeSet};

use agent_core::{
    AbortSignal, LanguageModel, StreamEvent, ToolCall, ToolExecutionContext, ToolExecutor,
    ToolOutputDelta, ToolResult,
};
use serde_json::json;
use session_tape::TapeEntry;

use crate::{RuntimeEvent, ToolInvocationLifecycle, ToolInvocationOutcome};

use super::{
    AgentRuntime, RuntimeError,
    helpers::{PreviousToolCall, build_tool_source_entry_ids, tool_call_signature},
};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn execute_tool_call(
        &mut self,
        turn_id: &str,
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: &ToolCall,
        seen_tool_calls: &mut BTreeMap<String, PreviousToolCall>,
        source_entry_ids: &mut Vec<u64>,
        on_delta: &mut dyn FnMut(StreamEvent),
    ) -> ToolInvocationLifecycle {
        let available_tool_names = self
            .visible_tools()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();
        let call_signature = tool_call_signature(call);

        if let Some(previous) = seen_tool_calls.get(&call_signature) {
            return self.record_failed_tool_call(
                turn_id,
                assistant_entry_id,
                tool_call_entry_id,
                call,
                source_entry_ids,
                "tool_call_skipped_duplicate",
                RuntimeError::duplicate_tool_call(call, previous),
                on_delta,
            );
        }

        if !available_tool_names.contains(&call.tool_name) {
            let runtime_error = RuntimeError::tool_unavailable(call.tool_name.clone());
            let lifecycle = self.record_failed_tool_call(
                turn_id,
                assistant_entry_id,
                tool_call_entry_id,
                call,
                source_entry_ids,
                "tool_call_rejected",
                runtime_error,
                on_delta,
            );
            seen_tool_calls
                .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
            return lifecycle;
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
            },
        ) {
            Ok(result) => {
                if result.invocation_id != call.invocation_id || result.tool_name != call.tool_name
                {
                    let runtime_error = RuntimeError::tool_result_mismatch(call, &result);
                    let lifecycle = self.record_failed_tool_call(
                        turn_id,
                        assistant_entry_id,
                        tool_call_entry_id,
                        call,
                        source_entry_ids,
                        "tool_result_rejected",
                        runtime_error,
                        on_delta,
                    );
                    seen_tool_calls
                        .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
                    return lifecycle;
                }

                let tool_result_entry_id =
                    self.tape.append_entry(TapeEntry::tool_result(&result).with_run_id(turn_id));
                source_entry_ids.push(tool_result_entry_id);
                let tool_result_event_id = self.tape.append_entry(
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
                );
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
                let lifecycle = ToolInvocationLifecycle { call: call.clone(), outcome };
                seen_tool_calls
                    .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
                lifecycle
            }
            Err(error) => {
                let lifecycle = self.record_failed_tool_call(
                    turn_id,
                    assistant_entry_id,
                    tool_call_entry_id,
                    call,
                    source_entry_ids,
                    "tool_call_failed",
                    RuntimeError::tool(error),
                    on_delta,
                );
                seen_tool_calls
                    .insert(call_signature, PreviousToolCall::from_outcome(&lifecycle.outcome));
                lifecycle
            }
        }
    }

    fn record_failed_tool_call(
        &mut self,
        turn_id: &str,
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: &ToolCall,
        source_entry_ids: &mut Vec<u64>,
        event_name: &str,
        runtime_error: RuntimeError,
        on_delta: &mut dyn FnMut(StreamEvent),
    ) -> ToolInvocationLifecycle {
        let failure_message = runtime_error.to_string();
        let failed_result = ToolResult::from_call(call, failure_message.clone());
        let tool_result_entry_id =
            self.tape.append_entry(TapeEntry::tool_result(&failed_result).with_run_id(turn_id));
        source_entry_ids.push(tool_result_entry_id);
        let failure_event_id = self.tape.append_entry(
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
        );
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
        ToolInvocationLifecycle { call: call.clone(), outcome }
    }
}
