use agent_core::{CompletionStopReason, LanguageModel, Message, Role, StreamEvent, ToolExecutor};
use session_tape::TapeEntry;

use crate::{RuntimeEvent, TurnControl, TurnOutput};

use super::super::{
    AgentRuntime, RuntimeError,
    compress::is_context_length_error,
    helpers::{next_turn_id, now_timestamp_ms},
};
use super::types::TurnBuffers;

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    fn fail_turn(
        &mut self,
        turn_id: &str,
        started_at_ms: u64,
        user_message: &str,
        buffers: &mut TurnBuffers,
        runtime_error: RuntimeError,
    ) -> Result<TurnOutput, RuntimeError> {
        self.record_turn_failure(
            buffers.failure_context(turn_id, started_at_ms, user_message),
            runtime_error.clone(),
        )?;
        Err(runtime_error)
    }

    pub async fn handle_turn_streaming(
        &mut self,
        user_input: impl Into<String>,
        control: TurnControl,
        mut on_delta: impl FnMut(StreamEvent) + Send,
    ) -> Result<TurnOutput, RuntimeError> {
        let abort_signal = control.abort_signal();
        let turn_id = next_turn_id();
        let started_at_ms = now_timestamp_ms();
        let user_input = user_input.into();

        let mut llm_step_index = 0_u32;

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        self.maybe_auto_compress_current_context(&turn_id, &mut llm_step_index).await;

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
                return self.fail_turn(
                    &turn_id,
                    started_at_ms,
                    &user_message.content,
                    &mut buffers,
                    RuntimeError::cancelled(),
                );
            }

            let request = self.build_completion_request(&turn_id, "completion", llm_step_index);
            let llm_trace_context = request.trace_context.clone();
            llm_step_index = llm_step_index.saturating_add(1);
            let completion = match self
                .model
                .complete_streaming(request, &abort_signal, &mut |event| {
                    buffers.record_stream_event(&event);
                    on_delta(event);
                })
                .await
            {
                Ok(completion) => {
                    self.last_input_tokens =
                        completion.usage.as_ref().map(|usage| usage.input_tokens as u64);
                    completion
                }
                Err(error) => {
                    if M::is_cancelled_error(&error) {
                        self.flush_streamed_partial_segments(&turn_id, &mut buffers)?;
                        return self.fail_turn(
                            &turn_id,
                            started_at_ms,
                            &user_message.content,
                            &mut buffers,
                            RuntimeError::cancelled(),
                        );
                    }
                    if !already_compressed && is_context_length_error(&error.to_string()) {
                        already_compressed = true;
                        if self.compress_context(Some(&turn_id), llm_step_index).await.is_ok() {
                            llm_step_index = llm_step_index.saturating_add(1);
                            continue;
                        }
                    }
                    return self.fail_turn(
                        &turn_id,
                        started_at_ms,
                        &user_message.content,
                        &mut buffers,
                        RuntimeError::model(error),
                    );
                }
            };

            if abort_signal.is_aborted() {
                return self.fail_turn(
                    &turn_id,
                    started_at_ms,
                    &user_message.content,
                    &mut buffers,
                    RuntimeError::cancelled(),
                );
            }

            if let Err(runtime_error) = self.validate_completion_stop_reason(&completion) {
                return self.fail_turn(
                    &turn_id,
                    started_at_ms,
                    &user_message.content,
                    &mut buffers,
                    runtime_error,
                );
            }

            let assistant_text = completion.plain_text();
            if !assistant_text.is_empty()
                && buffers.last_assistant_text.is_none()
                && buffers.streamed_assistant_text == assistant_text
            {
                self.flush_streamed_partial_segments(&turn_id, &mut buffers)?;
            }
            let saw_tool_calls = match self
                .process_completion_segments(
                    &turn_id,
                    llm_trace_context.as_ref(),
                    &completion,
                    &mut buffers,
                    &abort_signal,
                    &mut on_delta,
                )
                .await
            {
                Ok(value) => value,
                Err(runtime_error) => {
                    return self.fail_turn(
                        &turn_id,
                        started_at_ms,
                        &user_message.content,
                        &mut buffers,
                        runtime_error,
                    );
                }
            };

            match completion.stop_reason {
                CompletionStopReason::ToolUse => {
                    if !saw_tool_calls {
                        return self.fail_turn(
                            &turn_id,
                            started_at_ms,
                            &user_message.content,
                            &mut buffers,
                            RuntimeError::stop_reason_mismatch(&completion.stop_reason),
                        );
                    }
                }
                _ => {
                    if saw_tool_calls {
                        return self.fail_turn(
                            &turn_id,
                            started_at_ms,
                            &user_message.content,
                            &mut buffers,
                            RuntimeError::stop_reason_mismatch(&completion.stop_reason),
                        );
                    }

                    self.finish_success_turn(buffers.into_success_context(
                        turn_id.clone(),
                        started_at_ms,
                        user_message.content,
                        completion.usage.clone(),
                    ))?;
                    self.maybe_auto_compress_current_context(&turn_id, &mut llm_step_index).await;

                    return Ok(TurnOutput {
                        assistant_text,
                        completion,
                        visible_tools: self.visible_tools(),
                    });
                }
            }
        }
    }

    async fn maybe_auto_compress_current_context(&mut self, turn_id: &str, step_index: &mut u32) {
        if let Some(ratio) = self.context_pressure_ratio()
            && ratio >= self.context_pressure_threshold
            && self.compress_context(Some(turn_id), *step_index).await.is_ok()
        {
            *step_index = step_index.saturating_add(1);
        }
    }
}
