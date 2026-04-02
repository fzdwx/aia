use agent_core::{AbortSignal, CompletionRequest, ConversationItem, LanguageModel, ToolExecutor};
use serde_json::json;

use crate::RuntimeEvent;

use super::{AgentRuntime, RuntimeError, helpers::build_llm_trace_context};

const MIN_ENTRIES_FOR_COMPRESSION: usize = 4;
const SUMMARY_OUTPUT_RATIO: f64 = 0.20;
const SUMMARY_OUTPUT_FALLBACK: u32 = 16384;

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) async fn compress_context(
        &mut self,
        turn_id: Option<&str>,
        step_index: u32,
    ) -> Result<(), RuntimeError> {
        let view = self.tape.default_view();
        let (conversation, compressed_until_entry_id) =
            self.compression_conversation(turn_id, view.conversation);
        if conversation.len() < MIN_ENTRIES_FOR_COMPRESSION {
            return Ok(());
        }

        let summary_max_tokens = self
            .model_identity
            .limit
            .as_ref()
            .and_then(|l| l.output)
            .map(|o| (o as f64 * SUMMARY_OUTPUT_RATIO) as u32)
            .unwrap_or(SUMMARY_OUTPUT_FALLBACK)
            .max(1);

        let request = self.prepare_request_with_hooks(
            turn_id.unwrap_or("compression"),
            "compression",
            step_index,
            CompletionRequest {
                model: self.model_identity.clone(),
                instructions: Some(agent_prompts::handoff_summary(summary_max_tokens)),
                conversation,
                max_output_tokens: Some(summary_max_tokens),
                available_tools: Vec::new(),
                parallel_tool_calls: Some(false),
                prompt_cache: None,
                user_agent: None,
                timeout: self.request_timeout.clone(),
                trace_context: turn_id.map(|turn_id| {
                    build_llm_trace_context(
                        self.session_id.as_deref(),
                        turn_id,
                        turn_id,
                        "compression",
                        step_index,
                    )
                }),
            },
        )?;

        let completion =
            match self.model.complete_streaming(request, &AbortSignal::new(), &mut |_| {}).await {
                Ok(completion) => completion,
                Err(error) if turn_id.is_some() && is_context_length_error(&error.to_string()) => {
                    let (fallback_conversation, fallback_compressed_until_entry_id) =
                        self.fallback_compression_conversation(turn_id);
                    if fallback_conversation.len() < MIN_ENTRIES_FOR_COMPRESSION {
                        return Err(RuntimeError::model(error));
                    }
                    let fallback_request = self.prepare_request_with_hooks(
                        turn_id.unwrap_or("compression"),
                        "compression",
                        step_index,
                        CompletionRequest {
                            model: self.model_identity.clone(),
                            instructions: Some(agent_prompts::handoff_summary(summary_max_tokens)),
                            conversation: fallback_conversation,
                            max_output_tokens: Some(summary_max_tokens),
                            available_tools: Vec::new(),
                            parallel_tool_calls: Some(false),
                            prompt_cache: None,
                            user_agent: None,
                            timeout: self.request_timeout.clone(),
                            trace_context: turn_id.map(|turn_id| {
                                build_llm_trace_context(
                                    self.session_id.as_deref(),
                                    turn_id,
                                    turn_id,
                                    "compression",
                                    step_index,
                                )
                            }),
                        },
                    )?;
                    let completion = self
                        .model
                        .complete_streaming(fallback_request, &AbortSignal::new(), &mut |_| {})
                        .await
                        .map_err(RuntimeError::model)?;
                    let summary = completion.plain_text();
                    self.record_handoff_at(
                        fallback_compressed_until_entry_id,
                        "context_compression",
                        json!({
                            "summary": summary,
                            "compressed_until_entry_id": fallback_compressed_until_entry_id,
                        }),
                        "system",
                    )?;
                    self.publish_event(RuntimeEvent::ContextCompressed { summary });
                    return Ok(());
                }
                Err(error) => return Err(RuntimeError::model(error)),
            };
        let summary = completion.plain_text();

        self.record_handoff_at(
            compressed_until_entry_id,
            "context_compression",
            json!({
                "summary": summary,
                "compressed_until_entry_id": compressed_until_entry_id,
            }),
            "system",
        )?;

        self.publish_event(RuntimeEvent::ContextCompressed { summary });
        Ok(())
    }
}

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    fn compression_conversation(
        &self,
        turn_id: Option<&str>,
        conversation: Vec<ConversationItem>,
    ) -> (Vec<ConversationItem>, u64) {
        if turn_id.is_none() {
            let compressed_until_entry_id =
                self.tape.entries().last().map(|entry| entry.id).unwrap_or(0);
            return (conversation, compressed_until_entry_id);
        }

        let cutoff = conversation
            .iter()
            .rposition(|item| item.as_tool_call().is_none() && item.as_tool_result().is_none());

        match cutoff {
            Some(index) => {
                let trimmed = conversation.into_iter().take(index + 1).collect::<Vec<_>>();
                let compressed_until_entry_id =
                    self.tape.default_view().entries.get(index).map(|entry| entry.id).unwrap_or(0);
                (trimmed, compressed_until_entry_id)
            }
            None => (Vec::new(), 0),
        }
    }

    fn fallback_compression_conversation(
        &self,
        turn_id: Option<&str>,
    ) -> (Vec<ConversationItem>, u64) {
        let Some(turn_id) = turn_id else {
            return (Vec::new(), 0);
        };
        let Some(last_usage_entry_id) = self.last_usage_entry_id else {
            return (Vec::new(), 0);
        };

        let conversation = self
            .tape
            .entries()
            .iter()
            .filter(|entry| entry.id <= last_usage_entry_id)
            .filter(|entry| {
                entry.meta.get("run_id").and_then(|value| value.as_str()) != Some(turn_id)
                    || entry.as_message().is_some()
            })
            .filter_map(|entry| {
                entry
                    .as_message()
                    .map(ConversationItem::Message)
                    .or_else(|| entry.as_tool_call().map(ConversationItem::ToolCall))
                    .or_else(|| entry.as_tool_result().map(ConversationItem::ToolResult))
            })
            .collect();
        (conversation, last_usage_entry_id)
    }
}

pub(super) fn is_context_length_error(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("context_length_exceeded")
        || lowered.contains("maximum context length")
        || lowered.contains("longer than the model's context length")
        || lowered.contains("longer than the models context length")
        || lowered.contains("too many tokens")
        || lowered.contains("context window")
}

#[cfg(test)]
#[path = "../../tests/runtime/compress/mod.rs"]
mod tests;
