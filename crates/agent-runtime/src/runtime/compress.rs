use agent_core::{AbortSignal, CompletionRequest, LanguageModel, ToolExecutor};
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
        if view.conversation.len() < MIN_ENTRIES_FOR_COMPRESSION {
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
                conversation: view.conversation,
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
            .complete_streaming(request, &AbortSignal::new(), &mut |_| {})
            .await
            .map_err(RuntimeError::model)?;
        let summary = completion.plain_text();

        self.record_handoff("context_compression", json!({ "summary": summary }), "system")?;

        self.publish_event(RuntimeEvent::ContextCompressed { summary });
        Ok(())
    }
}

pub(super) fn is_context_length_error(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("context_length_exceeded")
        || lowered.contains("maximum context length")
        || lowered.contains("too many tokens")
        || lowered.contains("context window")
}

#[cfg(test)]
#[path = "../../tests/runtime/compress/mod.rs"]
mod tests;
