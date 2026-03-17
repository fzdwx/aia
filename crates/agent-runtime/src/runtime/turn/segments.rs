use agent_core::{
    AbortSignal, Completion, CompletionSegment, CompletionStopReason, LanguageModel,
    LlmTraceRequestContext, Message, Role, StreamEvent, ToolExecutor,
};
use session_tape::TapeEntry;

use crate::{RuntimeEvent, TurnBlock};

use super::super::{AgentRuntime, RuntimeError, tool_calls::ExecuteToolCallContext};
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
                    let invocation = self
                        .execute_tool_call(
                            ExecuteToolCallContext::new(
                                turn_id,
                                llm_trace_context,
                                assistant_entry_id,
                                tool_call_entry_id,
                                call,
                                &mut buffers.seen_tool_calls,
                                &mut buffers.source_entry_ids,
                                abort_signal.clone(),
                            ),
                            on_delta,
                        )
                        .await?;
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
