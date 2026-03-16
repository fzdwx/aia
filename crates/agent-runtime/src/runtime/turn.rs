use std::collections::BTreeMap;

use agent_core::{
    AbortSignal, Completion, CompletionSegment, CompletionStopReason, CompletionUsage,
    LanguageModel, LlmTraceRequestContext, Message, Role, StreamEvent, ToolExecutor,
};
use session_tape::TapeEntry;

use crate::{RuntimeEvent, ToolInvocationLifecycle, TurnBlock, TurnControl, TurnOutput};

use super::{
    AgentRuntime, RuntimeError,
    compress::is_context_length_error,
    helpers::{next_turn_id, now_timestamp_ms},
};

struct TurnBuffers {
    source_entry_ids: Vec<u64>,
    aggregated_thinking: String,
    streamed_thinking: String,
    tool_invocations: Vec<ToolInvocationLifecycle>,
    blocks: Vec<TurnBlock>,
    last_assistant_text: Option<String>,
    streamed_assistant_text: String,
    seen_tool_calls: BTreeMap<String, super::helpers::PreviousToolCall>,
}

impl TurnBuffers {
    fn new(user_entry_id: u64) -> Self {
        Self {
            source_entry_ids: vec![user_entry_id],
            aggregated_thinking: String::new(),
            streamed_thinking: String::new(),
            tool_invocations: Vec::new(),
            blocks: Vec::new(),
            last_assistant_text: None,
            streamed_assistant_text: String::new(),
            seen_tool_calls: BTreeMap::new(),
        }
    }

    fn thinking(&self) -> Option<String> {
        if self.aggregated_thinking.is_empty() {
            None
        } else {
            Some(self.aggregated_thinking.clone())
        }
    }

    fn record_stream_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ThinkingDelta { text } => {
                self.streamed_thinking.push_str(text);
            }
            StreamEvent::TextDelta { text } => {
                self.streamed_assistant_text.push_str(text);
            }
            StreamEvent::ToolCallDetected { .. }
            | StreamEvent::ToolCallStarted { .. }
            | StreamEvent::ToolOutputDelta { .. }
            | StreamEvent::ToolCallCompleted { .. }
            | StreamEvent::Log { .. }
            | StreamEvent::Done => {}
        }
    }
}

pub(super) struct TurnCompletionSummary {
    pub assistant_message: Option<String>,
    pub thinking: Option<String>,
    pub usage: Option<CompletionUsage>,
}

pub(super) struct TurnSuccessContext {
    pub turn_id: String,
    pub started_at_ms: u64,
    pub source_entry_ids: Vec<u64>,
    pub user_message: String,
    pub blocks: Vec<TurnBlock>,
    pub tool_invocations: Vec<ToolInvocationLifecycle>,
    pub summary: TurnCompletionSummary,
}

pub(super) struct TurnFailureContext<'a> {
    pub turn_id: &'a str,
    pub started_at_ms: u64,
    pub user_message: &'a str,
    pub source_entry_ids: &'a mut Vec<u64>,
    pub blocks: &'a [TurnBlock],
    pub assistant_message: Option<String>,
    pub aggregated_thinking: &'a str,
    pub tool_invocations: &'a [ToolInvocationLifecycle],
}

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub fn handle_turn(
        &mut self,
        user_input: impl Into<String>,
    ) -> Result<TurnOutput, RuntimeError> {
        self.handle_turn_streaming(user_input, |_| {})
    }

    pub fn handle_turn_streaming(
        &mut self,
        user_input: impl Into<String>,
        on_delta: impl FnMut(StreamEvent),
    ) -> Result<TurnOutput, RuntimeError> {
        self.handle_turn_streaming_with_control(
            user_input,
            TurnControl::new(AbortSignal::new()),
            on_delta,
        )
    }

    pub fn handle_turn_streaming_with_control(
        &mut self,
        user_input: impl Into<String>,
        control: TurnControl,
        mut on_delta: impl FnMut(StreamEvent),
    ) -> Result<TurnOutput, RuntimeError> {
        let abort_signal = control.abort_signal();
        let turn_id = next_turn_id();
        let started_at_ms = now_timestamp_ms();
        let user_input = user_input.into();

        let mut llm_step_index = 0_u32;

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        self.maybe_auto_compress_current_context(&turn_id, &mut llm_step_index);

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        let user_message = Message::new(Role::User, user_input);
        let user_entry_id =
            self.append_tape_entry(TapeEntry::message(&user_message).with_run_id(&turn_id))?;
        let mut buffers = TurnBuffers::new(user_entry_id);
        self.publish_event(RuntimeEvent::UserMessage { content: user_message.content.clone() });

        let mut already_compressed = false;

        loop {
            if abort_signal.is_aborted() {
                let runtime_error = RuntimeError::cancelled();
                self.record_turn_failure(
                    TurnFailureContext {
                        turn_id: &turn_id,
                        started_at_ms,
                        user_message: &user_message.content,
                        source_entry_ids: &mut buffers.source_entry_ids,
                        blocks: &buffers.blocks,
                        assistant_message: buffers.last_assistant_text.clone(),
                        aggregated_thinking: buffers.aggregated_thinking.as_str(),
                        tool_invocations: &buffers.tool_invocations,
                    },
                    runtime_error.clone(),
                )?;
                return Err(runtime_error);
            }

            let request = self.build_completion_request(&turn_id, "completion", llm_step_index);
            let llm_trace_context = request.trace_context.clone();
            llm_step_index = llm_step_index.saturating_add(1);
            let completion = match self.model.complete_streaming_with_abort(
                request,
                &abort_signal,
                &mut |event| {
                    buffers.record_stream_event(&event);
                    on_delta(event);
                },
            ) {
                Ok(completion) => {
                    self.last_input_tokens = completion.usage.as_ref().map(|usage| usage.input_tokens);
                    completion
                }
                Err(error) => {
                    if M::is_cancelled_error(&error) {
                        let runtime_error = RuntimeError::cancelled();
                        self.flush_streamed_partial_segments(&turn_id, &mut buffers)?;
                        self.record_turn_failure(
                            TurnFailureContext {
                                turn_id: &turn_id,
                                started_at_ms,
                                user_message: &user_message.content,
                                source_entry_ids: &mut buffers.source_entry_ids,
                                blocks: &buffers.blocks,
                                assistant_message: buffers.last_assistant_text.clone(),
                                aggregated_thinking: buffers.aggregated_thinking.as_str(),
                                tool_invocations: &buffers.tool_invocations,
                            },
                            runtime_error.clone(),
                        )?;
                        return Err(runtime_error);
                    }
                    if !already_compressed && is_context_length_error(&error.to_string()) {
                        already_compressed = true;
                        if self.compress_context(Some(&turn_id), llm_step_index).is_ok() {
                            llm_step_index = llm_step_index.saturating_add(1);
                            continue;
                        }
                    }
                    let runtime_error = RuntimeError::model(error);
                    self.record_turn_failure(
                        TurnFailureContext {
                            turn_id: &turn_id,
                            started_at_ms,
                            user_message: &user_message.content,
                            source_entry_ids: &mut buffers.source_entry_ids,
                            blocks: &buffers.blocks,
                            assistant_message: buffers.last_assistant_text.clone(),
                            aggregated_thinking: buffers.aggregated_thinking.as_str(),
                            tool_invocations: &buffers.tool_invocations,
                        },
                        runtime_error.clone(),
                    )?;
                    return Err(runtime_error);
                }
            };

            if abort_signal.is_aborted() {
                let runtime_error = RuntimeError::cancelled();
                self.record_turn_failure(
                    TurnFailureContext {
                        turn_id: &turn_id,
                        started_at_ms,
                        user_message: &user_message.content,
                        source_entry_ids: &mut buffers.source_entry_ids,
                        blocks: &buffers.blocks,
                        assistant_message: buffers.last_assistant_text.clone(),
                        aggregated_thinking: buffers.aggregated_thinking.as_str(),
                        tool_invocations: &buffers.tool_invocations,
                    },
                    runtime_error.clone(),
                )?;
                return Err(runtime_error);
            }

            if let Err(runtime_error) = self.validate_completion_stop_reason(&completion) {
                self.record_turn_failure(
                    TurnFailureContext {
                        turn_id: &turn_id,
                        started_at_ms,
                        user_message: &user_message.content,
                        source_entry_ids: &mut buffers.source_entry_ids,
                        blocks: &buffers.blocks,
                        assistant_message: buffers.last_assistant_text.clone(),
                        aggregated_thinking: buffers.aggregated_thinking.as_str(),
                        tool_invocations: &buffers.tool_invocations,
                    },
                    runtime_error.clone(),
                )?;
                return Err(runtime_error);
            }

            let assistant_text = completion.plain_text();
            if !assistant_text.is_empty()
                && buffers.last_assistant_text.is_none()
                && buffers.streamed_assistant_text == assistant_text
            {
                self.flush_streamed_partial_segments(&turn_id, &mut buffers)?;
            }
            let saw_tool_calls = match self.process_completion_segments(
                &turn_id,
                llm_trace_context.as_ref(),
                &completion,
                &mut buffers,
                &abort_signal,
                &mut on_delta,
            ) {
                Ok(value) => value,
                Err(runtime_error) => {
                    self.record_turn_failure(
                        TurnFailureContext {
                            turn_id: &turn_id,
                            started_at_ms,
                            user_message: &user_message.content,
                            source_entry_ids: &mut buffers.source_entry_ids,
                            blocks: &buffers.blocks,
                            assistant_message: buffers.last_assistant_text.clone(),
                            aggregated_thinking: buffers.aggregated_thinking.as_str(),
                            tool_invocations: &buffers.tool_invocations,
                        },
                        runtime_error.clone(),
                    )?;
                    return Err(runtime_error);
                }
            };

            match completion.stop_reason {
                CompletionStopReason::ToolUse => {
                    if !saw_tool_calls {
                        let runtime_error =
                            RuntimeError::stop_reason_mismatch(&completion.stop_reason);
                        self.record_turn_failure(
                            TurnFailureContext {
                                turn_id: &turn_id,
                                started_at_ms,
                                user_message: &user_message.content,
                                source_entry_ids: &mut buffers.source_entry_ids,
                                blocks: &buffers.blocks,
                                assistant_message: buffers.last_assistant_text.clone(),
                                aggregated_thinking: buffers.aggregated_thinking.as_str(),
                                tool_invocations: &buffers.tool_invocations,
                            },
                            runtime_error.clone(),
                        )?;
                        return Err(runtime_error);
                    }
                }
                _ => {
                    if saw_tool_calls {
                        let runtime_error =
                            RuntimeError::stop_reason_mismatch(&completion.stop_reason);
                        self.record_turn_failure(
                            TurnFailureContext {
                                turn_id: &turn_id,
                                started_at_ms,
                                user_message: &user_message.content,
                                source_entry_ids: &mut buffers.source_entry_ids,
                                blocks: &buffers.blocks,
                                assistant_message: buffers.last_assistant_text.clone(),
                                aggregated_thinking: buffers.aggregated_thinking.as_str(),
                                tool_invocations: &buffers.tool_invocations,
                            },
                            runtime_error.clone(),
                        )?;
                        return Err(runtime_error);
                    }

                    let thinking = buffers.thinking();
                    self.finish_success_turn(TurnSuccessContext {
                        turn_id: turn_id.clone(),
                        started_at_ms,
                        source_entry_ids: buffers.source_entry_ids,
                        user_message: user_message.content,
                        blocks: buffers.blocks,
                        tool_invocations: buffers.tool_invocations,
                        summary: TurnCompletionSummary {
                            assistant_message: buffers.last_assistant_text.clone(),
                            thinking,
                            usage: completion.usage.clone(),
                        },
                    })?;
                    self.maybe_auto_compress_current_context(&turn_id, &mut llm_step_index);

                    return Ok(TurnOutput {
                        assistant_text,
                        completion,
                        visible_tools: self.visible_tools(),
                    });
                }
            }
        }
    }

    fn maybe_auto_compress_current_context(&mut self, turn_id: &str, step_index: &mut u32) {
        if let Some(ratio) = self.context_pressure_ratio()
            && ratio >= self.context_pressure_threshold
            && self.compress_context(Some(turn_id), *step_index).is_ok()
        {
            *step_index = step_index.saturating_add(1);
        }
    }

    fn validate_completion_stop_reason(&self, completion: &Completion) -> Result<(), RuntimeError> {
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

    fn process_completion_segments(
        &mut self,
        turn_id: &str,
        llm_trace_context: Option<&LlmTraceRequestContext>,
        completion: &Completion,
        buffers: &mut TurnBuffers,
        abort_signal: &AbortSignal,
        on_delta: &mut dyn FnMut(StreamEvent),
    ) -> Result<bool, RuntimeError> {
        self.flush_streamed_partial_segments(turn_id, buffers)?;
        let mut assistant_entry_id = None;
        let mut saw_tool_calls = false;

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
                    if buffers.tool_invocations.len() >= self.max_tool_calls_per_turn {
                        return Err(RuntimeError::tool_call_limit(self.max_tool_calls_per_turn));
                    }
                    if abort_signal.is_aborted() {
                        return Err(RuntimeError::cancelled());
                    }
                    saw_tool_calls = true;
                    let tool_call_entry_id =
                        self.append_tape_entry(TapeEntry::tool_call(call).with_run_id(turn_id))?;
                    buffers.source_entry_ids.push(tool_call_entry_id);
                    let invocation = self.execute_tool_call(
                        super::tool_calls::ExecuteToolCallContext {
                            turn_id,
                            parent_trace_context: llm_trace_context,
                            assistant_entry_id,
                            tool_call_entry_id,
                            call,
                            seen_tool_calls: &mut buffers.seen_tool_calls,
                            source_entry_ids: &mut buffers.source_entry_ids,
                            abort_signal: abort_signal.clone(),
                        },
                        on_delta,
                    )?;
                    buffers.blocks.push(TurnBlock::ToolInvocation {
                        invocation: Box::new(invocation.clone()),
                    });
                    buffers.tool_invocations.push(invocation);
                }
                CompletionSegment::Thinking(_) | CompletionSegment::Text(_) => {}
            }
        }

        Ok(saw_tool_calls)
    }

    fn flush_streamed_partial_segments(
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
            let entry_id =
                self.append_tape_entry(TapeEntry::message(&assistant_message).with_run_id(turn_id))?;
            buffers.source_entry_ids.push(entry_id);
            self.publish_event(RuntimeEvent::AssistantMessage { content: assistant_text.clone() });
            buffers.last_assistant_text = Some(assistant_text.clone());
            buffers.blocks.push(TurnBlock::Assistant { content: assistant_text });
        }

        Ok(())
    }
}
