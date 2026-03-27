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

fn parse_iso8601_utc_seconds(input: &str) -> Option<u64> {
    if input.len() != 20 || !input.ends_with('Z') {
        return None;
    }

    let year: i64 = input.get(0..4)?.parse().ok()?;
    let month: i64 = input.get(5..7)?.parse().ok()?;
    let day: i64 = input.get(8..10)?.parse().ok()?;
    let hour: i64 = input.get(11..13)?.parse().ok()?;
    let minute: i64 = input.get(14..16)?.parse().ok()?;
    let second: i64 = input.get(17..19)?.parse().ok()?;

    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 { adjusted_year } else { adjusted_year - 399 } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days_since_epoch = era * 146097 + day_of_era - 719468;
    let total_seconds = days_since_epoch * 86_400 + hour * 3_600 + minute * 60 + second;

    (total_seconds >= 0).then_some((total_seconds as u64) * 1000)
}

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

    fn restore_suspended_turn(
        &self,
        turn_id: &str,
    ) -> Result<(u64, String, TurnBuffers), RuntimeError> {
        let entries = self.tape().entries();
        let has_waiting_event = entries.iter().rev().any(|entry| {
            entry.event_name() == Some("turn_suspended")
                && entry.meta.get("run_id").and_then(|value| value.as_str()) == Some(turn_id)
        });
        if !has_waiting_event {
            return Err(RuntimeError::session("missing turn_suspended event"));
        }

        let mut started_at_ms = 0_u64;
        let mut user_message: Option<String> = None;
        let mut source_entry_ids = Vec::new();
        let mut aggregated_thinking = String::new();
        let mut tool_invocations = Vec::new();
        let mut blocks = Vec::new();
        let mut last_assistant_text = None;
        let mut pending_tool_calls = std::collections::BTreeMap::new();

        for entry in entries {
            let run_id = entry.meta.get("run_id").and_then(|value| value.as_str());
            if run_id != Some(turn_id) {
                continue;
            }

            source_entry_ids.push(entry.id);
            if started_at_ms == 0 {
                started_at_ms = parse_iso8601_utc_seconds(&entry.date).unwrap_or(0);
            }

            if let Some(message) = entry.as_message() {
                match message.role {
                    Role::User if user_message.is_none() => user_message = Some(message.content),
                    Role::Assistant => {
                        last_assistant_text = Some(message.content.clone());
                        blocks.push(crate::TurnBlock::Assistant { content: message.content });
                    }
                    Role::System | Role::Tool | Role::User => {}
                }
                continue;
            }

            if let Some(content) = entry.as_thinking() {
                aggregated_thinking.push_str(content);
                blocks.push(crate::TurnBlock::Thinking { content: content.to_string() });
                continue;
            }

            if let Some(call) = entry.as_tool_call() {
                pending_tool_calls.insert(call.invocation_id.clone(), call);
                continue;
            }

            if let Some(result) = entry.as_tool_result() {
                let call = pending_tool_calls.remove(&result.invocation_id).unwrap_or_else(|| {
                    agent_core::ToolCall::new(result.tool_name.clone())
                        .with_invocation_id(result.invocation_id.clone())
                });
                let invocation = crate::ToolInvocationLifecycle {
                    call,
                    started_at_ms,
                    finished_at_ms: started_at_ms,
                    trace_context: None,
                    outcome: crate::ToolInvocationOutcome::Succeeded { result },
                };
                blocks.push(crate::TurnBlock::ToolInvocation {
                    invocation: Box::new(invocation.clone()),
                });
                tool_invocations.push(invocation);
            }
        }

        let user_message = user_message
            .ok_or_else(|| RuntimeError::session("missing user message for waiting turn"))?;

        Ok((
            started_at_ms,
            user_message,
            TurnBuffers::from_restored_state(
                source_entry_ids,
                aggregated_thinking,
                tool_invocations,
                blocks,
                last_assistant_text,
            ),
        ))
    }

    async fn drive_turn_loop(
        &mut self,
        turn_id: String,
        started_at_ms: u64,
        user_message: Message,
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
                    &user_message.content,
                    &mut buffers,
                    RuntimeError::cancelled(),
                );
            }

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
                        &user_message.content,
                        &mut buffers,
                        runtime_error,
                    );
                }
            };

            if let CompletionProcessingResult::Suspended { request } = processing_result {
                self.finish_suspended_turn(
                    buffers.into_success_context(
                        turn_id.clone(),
                        started_at_ms,
                        user_message.content.clone(),
                        completion.usage.clone(),
                    ),
                    &request,
                )?;

                return Ok(TurnOutput {
                    assistant_text,
                    completion,
                    visible_tools: self.visible_tools(),
                });
            }

            let CompletionProcessingResult::Continue { saw_tool_calls } = processing_result else {
                unreachable!();
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
                        user_message.content.clone(),
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

    pub async fn handle_turn_streaming(
        &mut self,
        user_input: impl Into<String>,
        control: TurnControl,
        on_delta: impl FnMut(StreamEvent) + Send,
    ) -> Result<TurnOutput, RuntimeError> {
        let turn_id = next_turn_id();
        let started_at_ms = now_timestamp_ms();
        let user_input = self.rewrite_input(user_input.into())?;
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

        let user_message = Message::new(Role::User, user_input);
        let user_entry_id =
            self.append_tape_entry(TapeEntry::message(&user_message).with_run_id(&turn_id))?;
        let buffers = TurnBuffers::new(user_entry_id);
        self.publish_event(RuntimeEvent::UserMessage { content: user_message.content.clone() });
        self.notify_turn_start(&turn_id, &user_message.content);

        self.drive_turn_loop(
            turn_id,
            started_at_ms,
            user_message,
            control,
            buffers,
            llm_step_index,
            on_delta,
        )
        .await
    }

    pub async fn resume_turn_after_tool_result(
        &mut self,
        turn_id: &str,
        control: TurnControl,
        on_delta: impl FnMut(StreamEvent) + Send,
    ) -> Result<TurnOutput, RuntimeError> {
        self.ensure_agent_started()?;

        let (started_at_ms, user_message, buffers) = self.restore_suspended_turn(turn_id)?;
        let resumed_user_message = Message::new(Role::User, user_message);

        self.drive_turn_loop(
            turn_id.to_string(),
            started_at_ms,
            resumed_user_message,
            control,
            buffers,
            1,
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
}
