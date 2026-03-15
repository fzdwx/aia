use agent_core::{CompletionRequest, LanguageModel, ToolExecutor};
use serde_json::json;

use crate::RuntimeEvent;

use super::{AgentRuntime, RuntimeError, helpers::build_llm_trace_context};

const MIN_ENTRIES_FOR_COMPRESSION: usize = 4;

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn compress_context(
        &mut self,
        turn_id: Option<&str>,
        step_index: u32,
    ) -> Result<(), RuntimeError> {
        let view = self.tape.default_view();
        if view.conversation.len() < MIN_ENTRIES_FOR_COMPRESSION {
            return Ok(());
        }

        let request = CompletionRequest {
            model: self.model_identity.clone(),
            instructions: Some(agent_prompts::HANDOFF_SUMMARY.to_string()),
            conversation: view.conversation,
            max_output_tokens: Some(agent_prompts::HANDOFF_SUMMARY_MAX_OUTPUT_TOKENS),
            available_tools: Vec::new(),
            trace_context: turn_id.map(|turn_id| {
                build_llm_trace_context(turn_id, turn_id, "compression", step_index)
            }),
        };

        let completion = self.model.complete(request).map_err(RuntimeError::model)?;
        let summary = completion.plain_text();

        self.record_handoff("context_compression", json!({ "summary": summary }))?;

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
mod tests {
    use super::is_context_length_error;

    #[test]
    fn detects_openai_context_length_exceeded() {
        assert!(is_context_length_error("Error: context_length_exceeded - max tokens 128000"));
    }

    #[test]
    fn detects_maximum_context_length() {
        assert!(is_context_length_error("This model's maximum context length is 128000 tokens"));
    }

    #[test]
    fn detects_too_many_tokens() {
        assert!(is_context_length_error("Request has too many tokens"));
    }

    #[test]
    fn detects_context_window() {
        assert!(is_context_length_error("Input exceeds the context window limit"));
    }

    #[test]
    fn does_not_match_unrelated_errors() {
        assert!(!is_context_length_error("rate limit exceeded"));
        assert!(!is_context_length_error("internal server error"));
    }
}
