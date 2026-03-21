use agent_core::{
    AbortSignal, Completion, CompletionSegment, CompletionStopReason, LanguageModel,
    LlmTraceRequestContext, Message, Role, StreamEvent, ToolExecutionContext, ToolExecutor,
    ToolOutputDelta,
};
use futures::future::join_all;
use session_tape::TapeEntry;

use crate::{RuntimeEvent, TurnBlock};

use super::super::{
    AgentRuntime, RuntimeError,
    helpers::{build_tool_trace_context, now_timestamp_ms},
    tape_tools,
    tool_calls::{ExecuteToolCallContext, can_run_in_parallel},
};
use super::types::TurnBuffers;

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn validate_completion_stop_reason(
        &self,
        completion: &Completion,
    ) -> Result<(), RuntimeError> {
        let has_tool_use_segment = completion
            .segments
            .iter()
            .any(|segment| matches!(segment, CompletionSegment::ToolUse(_)));

        match completion.stop_reason {
            CompletionStopReason::ToolUse if !has_tool_use_segment => {
                Err(RuntimeError::stop_reason_mismatch(&completion.stop_reason))
            }
            CompletionStopReason::ToolUse => Ok(()),
            _ if has_tool_use_segment => {
                Err(RuntimeError::stop_reason_mismatch(&completion.stop_reason))
            }
            _ => Ok(()),
        }
    }

    pub(super) async fn process_completion_segments(
        &mut self,
        turn_id: &str,
        llm_trace_context: Option<&LlmTraceRequestContext>,
        completion: &Completion,
        buffers: &mut TurnBuffers,
        abort_signal: &AbortSignal,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<bool, RuntimeError> {
        self.flush_streamed_partial_segments(turn_id, buffers)?;
        let mut assistant_entry_id = None;
        let mut saw_tool_calls = false;
        let tool_calls_are_parallel = completion
            .segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::ToolUse(call) => Some(call),
                CompletionSegment::Thinking(_) | CompletionSegment::Text(_) => None,
            })
            .all(|call| can_run_in_parallel(call) && !tape_tools::is_runtime_tool(&call.tool_name));

        let mut parallel_calls = Vec::new();

        for segment in &completion.segments {
            match segment {
                CompletionSegment::Thinking(text) if !text.is_empty() => {
                    if buffers.aggregated_thinking == *text {
                        continue;
                    }
                    let thinking_entry_id =
                        self.append_tape_entry(TapeEntry::thinking(text).with_run_id(turn_id))?;
                    buffers.source_entry_ids.push(thinking_entry_id);
                    buffers.aggregated_thinking.push_str(text);
                    buffers.blocks.push(TurnBlock::Thinking { content: text.clone() });
                }
                CompletionSegment::Text(text) if !text.is_empty() => {
                    if buffers.last_assistant_text.as_deref() == Some(text.as_str()) {
                        continue;
                    }
                    let assistant_message = Message::new(Role::Assistant, text.clone());
                    let entry_id = self.append_tape_entry(
                        TapeEntry::message(&assistant_message).with_run_id(turn_id),
                    )?;
                    buffers.source_entry_ids.push(entry_id);
                    self.publish_event(RuntimeEvent::AssistantMessage { content: text.clone() });
                    buffers.last_assistant_text = Some(text.clone());
                    buffers.blocks.push(TurnBlock::Assistant { content: text.clone() });
                    assistant_entry_id = Some(entry_id);
                }
                CompletionSegment::ToolUse(call) => {
                    if buffers.tool_invocations.len() + parallel_calls.len()
                        >= self.max_tool_calls_per_turn
                    {
                        return Err(RuntimeError::tool_call_limit(self.max_tool_calls_per_turn));
                    }
                    if abort_signal.is_aborted() {
                        return Err(RuntimeError::cancelled());
                    }
                    saw_tool_calls = true;
                    let override_result = self.resolve_tool_call_override(turn_id, call)?;
                    let tool_call_entry_id =
                        self.append_tape_entry(TapeEntry::tool_call(call).with_run_id(turn_id))?;
                    buffers.source_entry_ids.push(tool_call_entry_id);

                    if tool_calls_are_parallel {
                        parallel_calls.push((
                            assistant_entry_id,
                            tool_call_entry_id,
                            call.clone(),
                            override_result,
                        ));
                    } else {
                        let mut context = ExecuteToolCallContext::new(
                            turn_id,
                            llm_trace_context,
                            assistant_entry_id,
                            tool_call_entry_id,
                            call,
                            &mut buffers.source_entry_ids,
                            abort_signal.clone(),
                        );
                        let invocation = match override_result {
                            Some(result) => {
                                on_delta(context.started_event());
                                self.commit_prepared_tool_call(
                                    &mut context,
                                    super::super::tool_calls::PreparedToolCallOutcome::Completed {
                                        started_at_ms: now_timestamp_ms(),
                                        tool_trace_context: llm_trace_context
                                            .map(|trace| build_tool_trace_context(trace, call)),
                                        result,
                                        failed: abort_signal.is_aborted(),
                                    },
                                    on_delta,
                                )?
                            }
                            None => self.execute_tool_call(context, on_delta).await?,
                        };
                        buffers.blocks.push(TurnBlock::ToolInvocation {
                            invocation: Box::new(invocation.clone()),
                        });
                        buffers.tool_invocations.push(invocation);
                    }
                }
                CompletionSegment::Thinking(_) | CompletionSegment::Text(_) => {}
            }
        }

        if tool_calls_are_parallel && !parallel_calls.is_empty() {
            let visible_tool_names = self
                .visible_tools()
                .into_iter()
                .map(|definition| definition.name)
                .collect::<std::collections::BTreeSet<_>>();
            let tools = self.tools.clone();
            let workspace_root = self.workspace_root.clone();
            let turn_id_owned = turn_id.to_string();
            let trace_context = llm_trace_context.cloned();

            let prepared_results = join_all(parallel_calls.iter().cloned().map(
                |(assistant_entry_id, tool_call_entry_id, call, override_result)| {
                    let tools = tools.clone();
                    let visible_tool_names = visible_tool_names.clone();
                    let workspace_root = workspace_root.clone();
                    let abort_signal = abort_signal.clone();
                    let turn_id_owned = turn_id_owned.clone();
                    let trace_context = trace_context.clone();
                    async move {
                        let started_at_ms = now_timestamp_ms();
                        let tool_trace_context = trace_context
                            .as_ref()
                            .map(|trace| build_tool_trace_context(trace, &call));

                        if let Some(result) = override_result {
                            return (
                                assistant_entry_id,
                                tool_call_entry_id,
                                call,
                                super::super::tool_calls::PreparedToolCallOutcome::Completed {
                                    started_at_ms,
                                    tool_trace_context,
                                    result,
                                    failed: abort_signal.is_aborted(),
                                },
                                Vec::<ToolOutputDelta>::new(),
                            );
                        }

                        if abort_signal.is_aborted() {
                            return (
                                assistant_entry_id,
                                tool_call_entry_id,
                                call,
                                super::super::tool_calls::PreparedToolCallOutcome::Failed {
                                    started_at_ms,
                                    tool_trace_context,
                                    event_name: "tool_call_cancelled",
                                    runtime_error: RuntimeError::cancelled(),
                                },
                                Vec::<ToolOutputDelta>::new(),
                            );
                        }

                        if !visible_tool_names.contains(&call.tool_name) {
                            let unavailable_name = call.tool_name.clone();
                            return (
                                assistant_entry_id,
                                tool_call_entry_id,
                                call,
                                super::super::tool_calls::PreparedToolCallOutcome::Failed {
                                    started_at_ms,
                                    tool_trace_context,
                                    event_name: "tool_call_rejected",
                                    runtime_error: RuntimeError::tool_unavailable(unavailable_name),
                                },
                                Vec::<ToolOutputDelta>::new(),
                            );
                        }

                        let mut deltas = Vec::<ToolOutputDelta>::new();
                        let prepared = match tools
                            .call(
                                &call,
                                &mut |delta| deltas.push(delta),
                                &ToolExecutionContext {
                                    run_id: turn_id_owned,
                                    workspace_root,
                                    abort: abort_signal.clone(),
                                    runtime: None,
                                },
                            )
                            .await
                        {
                            Ok(result) => {
                                if result.invocation_id != call.invocation_id
                                    || result.tool_name != call.tool_name
                                {
                                    super::super::tool_calls::PreparedToolCallOutcome::Failed {
                                        started_at_ms,
                                        tool_trace_context,
                                        event_name: "tool_result_rejected",
                                        runtime_error: RuntimeError::tool_result_mismatch(
                                            &call, &result,
                                        ),
                                    }
                                } else {
                                    super::super::tool_calls::PreparedToolCallOutcome::Completed {
                                        started_at_ms,
                                        tool_trace_context,
                                        result,
                                        failed: abort_signal.is_aborted(),
                                    }
                                }
                            }
                            Err(error) => {
                                super::super::tool_calls::PreparedToolCallOutcome::Failed {
                                    started_at_ms,
                                    tool_trace_context,
                                    event_name: "tool_call_failed",
                                    runtime_error: RuntimeError::tool(error),
                                }
                            }
                        };

                        (assistant_entry_id, tool_call_entry_id, call, prepared, deltas)
                    }
                },
            ))
            .await;

            let mut prepared_results = prepared_results;
            prepared_results.sort_by_key(|(_, tool_call_entry_id, _, _, _)| *tool_call_entry_id);

            for (assistant_entry_id, tool_call_entry_id, call, prepared, deltas) in prepared_results
            {
                on_delta(StreamEvent::ToolCallStarted {
                    invocation_id: call.invocation_id.clone(),
                    tool_name: call.tool_name.clone(),
                    arguments: call.arguments.clone(),
                });
                for delta in deltas {
                    on_delta(StreamEvent::ToolOutputDelta {
                        invocation_id: call.invocation_id.clone(),
                        stream: delta.stream,
                        text: delta.text,
                    });
                }
                let invocation = self.commit_prepared_tool_call(
                    &mut ExecuteToolCallContext::new(
                        turn_id,
                        llm_trace_context,
                        assistant_entry_id,
                        tool_call_entry_id,
                        &call,
                        &mut buffers.source_entry_ids,
                        abort_signal.clone(),
                    ),
                    prepared,
                    on_delta,
                )?;
                buffers
                    .blocks
                    .push(TurnBlock::ToolInvocation { invocation: Box::new(invocation.clone()) });
                buffers.tool_invocations.push(invocation);
            }
        }

        Ok(saw_tool_calls)
    }

    pub(super) fn flush_streamed_partial_segments(
        &mut self,
        turn_id: &str,
        buffers: &mut TurnBuffers,
    ) -> Result<(), RuntimeError> {
        if !buffers.streamed_thinking.is_empty() && buffers.aggregated_thinking.is_empty() {
            let thinking = std::mem::take(&mut buffers.streamed_thinking);
            let thinking_entry_id =
                self.append_tape_entry(TapeEntry::thinking(&thinking).with_run_id(turn_id))?;
            buffers.source_entry_ids.push(thinking_entry_id);
            buffers.aggregated_thinking.push_str(&thinking);
            buffers.blocks.push(TurnBlock::Thinking { content: thinking });
        }

        if !buffers.streamed_assistant_text.is_empty() && buffers.last_assistant_text.is_none() {
            let assistant_text = std::mem::take(&mut buffers.streamed_assistant_text);
            let assistant_message = Message::new(Role::Assistant, assistant_text.clone());
            let entry_id = self
                .append_tape_entry(TapeEntry::message(&assistant_message).with_run_id(turn_id))?;
            buffers.source_entry_ids.push(entry_id);
            self.publish_event(RuntimeEvent::AssistantMessage { content: assistant_text.clone() });
            buffers.last_assistant_text = Some(assistant_text.clone());
            buffers.blocks.push(TurnBlock::Assistant { content: assistant_text });
        }

        Ok(())
    }
}
