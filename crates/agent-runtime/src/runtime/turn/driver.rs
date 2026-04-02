use agent_core::{CompletionStopReason, LanguageModel, Message, Role, StreamEvent, ToolExecutor};
use session_tape::TapeEntry;

use crate::{RuntimeEvent, TurnControl, TurnOutput};

use super::super::{
    AgentRuntime, RuntimeError,
    compress::is_context_length_error,
    helpers::{next_turn_id, now_timestamp_ms},
};
use super::segments::CompletionProcessingResult;
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
        user_messages: &[String],
        buffers: &mut TurnBuffers,
        runtime_error: RuntimeError,
    ) -> Result<TurnOutput, RuntimeError> {
        self.record_turn_failure(
            buffers.failure_context(turn_id, started_at_ms, user_messages),
            runtime_error.clone(),
        )?;
        Err(runtime_error)
    }

    async fn drive_turn_loop(
        &mut self,
        turn_id: String,
        started_at_ms: u64,
        user_messages: Vec<String>,
        control: TurnControl,
        mut buffers: TurnBuffers,
        mut llm_step_index: u32,
        mut on_delta: impl FnMut(StreamEvent) + Send,
    ) -> Result<TurnOutput, RuntimeError> {
        let abort_signal = control.abort_signal();
        let mut already_compressed = false;

        loop {
            if abort_signal.is_aborted() {
                return self.fail_turn(
                    &turn_id,
                    started_at_ms,
                    &user_messages,
                    &mut buffers,
                    RuntimeError::cancelled(),
                );
            }

            self.maybe_auto_compress_before_completion(&turn_id, &mut llm_step_index).await?;

            let request = self.prepare_request_with_hooks(
                &turn_id,
                "completion",
                llm_step_index,
                self.build_completion_request(&turn_id, "completion", llm_step_index),
            )?;
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
                    self.last_usage_turn_id = Some(turn_id.clone());
                    self.last_usage_step_index = Some(llm_step_index);
                    self.last_usage_entry_id = self.tape.entries().last().map(|entry| entry.id);
                    completion
                }
                Err(error) => {
                    if M::is_cancelled_error(&error) {
                        self.flush_streamed_partial_segments(&turn_id, &mut buffers)?;
                        return self.fail_turn(
                            &turn_id,
                            started_at_ms,
                            &user_messages,
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
                        &user_messages,
                        &mut buffers,
                        RuntimeError::model(error),
                    );
                }
            };

            if abort_signal.is_aborted() {
                return self.fail_turn(
                    &turn_id,
                    started_at_ms,
                    &user_messages,
                    &mut buffers,
                    RuntimeError::cancelled(),
                );
            }

            if let Err(runtime_error) = self.validate_completion_stop_reason(&completion) {
                return self.fail_turn(
                    &turn_id,
                    started_at_ms,
                    &user_messages,
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
            let processing_result = match self
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
                        &user_messages,
                        &mut buffers,
                        runtime_error,
                    );
                }
            };

            let CompletionProcessingResult::Continue { saw_tool_calls } = processing_result;

            match completion.stop_reason {
                CompletionStopReason::ToolUse => {
                    if !saw_tool_calls {
                        return self.fail_turn(
                            &turn_id,
                            started_at_ms,
                            &user_messages,
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
                            &user_messages,
                            &mut buffers,
                            RuntimeError::stop_reason_mismatch(&completion.stop_reason),
                        );
                    }

                    self.finish_success_turn(buffers.into_success_context(
                        turn_id.clone(),
                        started_at_ms,
                        user_messages,
                        completion.usage.clone(),
                    ))?;

                    return Ok(TurnOutput {
                        assistant_text,
                        completion,
                        visible_tools: self.visible_tools(),
                    });
                }
            }
        }
    }

    /// 处理 turn，支持单条或多条用户消息
    ///
    /// 每条消息作为独立的 user message 追加到 tape，
    /// agent 能清晰看到这是多条独立的用户输入。
    pub async fn handle_turn_streaming(
        &mut self,
        user_inputs: Vec<String>,
        control: TurnControl,
        on_delta: impl FnMut(StreamEvent) + Send,
    ) -> Result<TurnOutput, RuntimeError> {
        if user_inputs.is_empty() {
            return Err(RuntimeError::session("no user messages provided"));
        }

        let turn_id = next_turn_id();
        let started_at_ms = now_timestamp_ms();
        let abort_signal = control.abort_signal();

        self.ensure_agent_started()?;

        let mut llm_step_index = 0_u32;

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        self.maybe_auto_compress_current_context(&turn_id, &mut llm_step_index).await;

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        // 追加所有 user message 到 tape，收集 entry IDs
        let mut user_entry_ids = Vec::with_capacity(user_inputs.len());
        for user_input in &user_inputs {
            let user_input = self.rewrite_input(user_input.clone())?;
            let user_message = Message::new(Role::User, user_input);
            let entry_id =
                self.append_tape_entry(TapeEntry::message(&user_message).with_run_id(&turn_id))?;
            user_entry_ids.push(entry_id);
            self.publish_event(RuntimeEvent::UserMessage { content: user_message.content.clone() });
        }

        let buffers = TurnBuffers::with_user_entries(user_entry_ids);

        // 使用所有消息作为预览
        let preview: String = user_inputs.join("\n");
        self.notify_turn_start(&turn_id, &preview);

        self.drive_turn_loop(
            turn_id,
            started_at_ms,
            user_inputs,
            control,
            buffers,
            llm_step_index,
            on_delta,
        )
        .await
    }

    async fn maybe_auto_compress_current_context(&mut self, turn_id: &str, step_index: &mut u32) {
        if let Some(ratio) = self.context_pressure_ratio()
            && ratio >= self.context_pressure_threshold
            && self.compress_context(Some(turn_id), *step_index).await.is_ok()
        {
            *step_index = step_index.saturating_add(1);
        }
    }

    async fn maybe_auto_compress_before_completion(
        &mut self,
        turn_id: &str,
        llm_step_index: &mut u32,
    ) -> Result<(), RuntimeError> {
        let should_compress = self.should_preflight_compress(turn_id)
            || self.should_preflight_compress_after_tool_result(turn_id);
        if !should_compress {
            return Ok(());
        }

        self.compress_context(Some(turn_id), *llm_step_index).await?;
        *llm_step_index = llm_step_index.saturating_add(1);
        Ok(())
    }

    fn should_preflight_compress_after_tool_result(&self, turn_id: &str) -> bool {
        let Some(last_input_tokens) = self.last_input_tokens else {
            return false;
        };
        if !self.exceeds_context_pressure_threshold(last_input_tokens) {
            return false;
        }

        self.current_turn_has_tool_result_since_usage(turn_id)
    }
}
