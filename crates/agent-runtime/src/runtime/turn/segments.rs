use agent_core::{
    AbortSignal, Completion, CompletionSegment, CompletionStopReason, LanguageModel,
    LlmTraceRequestContext, Message, Role, StreamEvent, ToolCall, ToolExecutionContext,
    ToolExecutor, ToolOutputDelta,
};
use futures::stream::{FuturesUnordered, StreamExt};
use session_tape::TapeEntry;

use crate::{RuntimeEvent, TurnBlock};

use super::super::{
    AgentRuntime, RuntimeError,
    helpers::{build_tool_trace_context, now_timestamp_ms},
    tool_calls::{ExecuteToolCallContext, can_run_in_parallel},
};
use super::types::TurnBuffers;

enum ParallelToolStreamMessage {
    Started {
        invocation_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        started_at_ms: u64,
    },
    Delta {
        invocation_id: String,
        stream: agent_core::ToolOutputStream,
        text: String,
    },
    Finished {
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: ToolCall,
        prepared: super::super::tool_calls::PreparedToolCallOutcome,
    },
}

pub(super) enum CompletionProcessingResult {
    Continue { saw_tool_calls: bool },
}

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
    ) -> Result<CompletionProcessingResult, RuntimeError> {
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
            .all(|call| {
                can_run_in_parallel(call)
                    && !self.tools.tool_requires_runtime_context(&call.tool_name)
            });

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
                                let started_at_ms = now_timestamp_ms();
                                on_delta(context.started_event(started_at_ms));
                                self.commit_prepared_tool_call(
                                    &mut context,
                                    super::super::tool_calls::PreparedToolCallOutcome::Completed {
                                        started_at_ms,
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
            let session_id = self.session_id.clone();
            let turn_id_owned = turn_id.to_string();
            let trace_context = llm_trace_context.cloned();
            let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut in_flight = FuturesUnordered::new();

            for (assistant_entry_id, tool_call_entry_id, call, override_result) in
                parallel_calls.iter().cloned()
            {
                let tools = tools.clone();
                let visible_tool_names = visible_tool_names.clone();
                let workspace_root = workspace_root.clone();
                let session_id = session_id.clone();
                let abort_signal = abort_signal.clone();
                let turn_id_owned = turn_id_owned.clone();
                let trace_context = trace_context.clone();
                let stream_tx = stream_tx.clone();

                in_flight.push(async move {
                    let started_at_ms = now_timestamp_ms();
                    let tool_trace_context =
                        trace_context.as_ref().map(|trace| build_tool_trace_context(trace, &call));
                    let _ = stream_tx.send(ParallelToolStreamMessage::Started {
                        invocation_id: call.invocation_id.clone(),
                        tool_name: call.tool_name.clone(),
                        arguments: call.arguments.clone(),
                        started_at_ms,
                    });

                    let prepared = if let Some(result) = override_result {
                        super::super::tool_calls::PreparedToolCallOutcome::Completed {
                            started_at_ms,
                            tool_trace_context,
                            result,
                            failed: abort_signal.is_aborted(),
                        }
                    } else if abort_signal.is_aborted() {
                        super::super::tool_calls::PreparedToolCallOutcome::Failed {
                            started_at_ms,
                            tool_trace_context,
                            event_name: "tool_call_cancelled",
                            runtime_error: RuntimeError::cancelled(),
                        }
                    } else if !visible_tool_names.contains(&call.tool_name) {
                        let unavailable_name = call.tool_name.clone();
                        super::super::tool_calls::PreparedToolCallOutcome::Failed {
                            started_at_ms,
                            tool_trace_context,
                            event_name: "tool_call_rejected",
                            runtime_error: RuntimeError::tool_unavailable(unavailable_name),
                        }
                    } else {
                        let invocation_id = call.invocation_id.clone();
                        match tools
                            .call(
                                &call,
                                &mut |delta: ToolOutputDelta| {
                                    let _ = stream_tx.send(ParallelToolStreamMessage::Delta {
                                        invocation_id: invocation_id.clone(),
                                        stream: delta.stream,
                                        text: delta.text,
                                    });
                                },
                                &ToolExecutionContext {
                                    run_id: turn_id_owned,
                                    session_id,
                                    workspace_root,
                                    abort: abort_signal.clone(),
                                    runtime: None,
                                    runtime_host: None,
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
                        }
                    };

                    let _ = stream_tx.send(ParallelToolStreamMessage::Finished {
                        assistant_entry_id,
                        tool_call_entry_id,
                        call,
                        prepared,
                    });
                });
            }
            drop(stream_tx);

            let mut completed_invocations = Vec::new();
            loop {
                tokio::select! {
                    Some(message) = stream_rx.recv() => {
                        match message {
                            ParallelToolStreamMessage::Started {
                                invocation_id,
                                tool_name,
                                arguments,
                                started_at_ms,
                            } => on_delta(StreamEvent::ToolCallStarted {
                                invocation_id,
                                tool_name,
                                arguments,
                                started_at_ms,
                            }),
                            ParallelToolStreamMessage::Delta {
                                invocation_id,
                                stream,
                                text,
                            } => on_delta(StreamEvent::ToolOutputDelta {
                                invocation_id,
                                stream,
                                text,
                            }),
                            ParallelToolStreamMessage::Finished {
                                assistant_entry_id,
                                tool_call_entry_id,
                                call,
                                prepared,
                            } => {
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
                                completed_invocations.push((tool_call_entry_id, invocation));
                            }
                        }
                    }
                    Some(_) = in_flight.next() => {}
                    else => break,
                }
            }

            completed_invocations.sort_by_key(|(tool_call_entry_id, _)| *tool_call_entry_id);
            for (_, invocation) in completed_invocations {
                buffers
                    .blocks
                    .push(TurnBlock::ToolInvocation { invocation: Box::new(invocation.clone()) });
                buffers.tool_invocations.push(invocation);
            }
        }

        Ok(CompletionProcessingResult::Continue { saw_tool_calls })
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
