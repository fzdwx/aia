use agent_core::{CompletionRequest, LanguageModel, ToolExecutor};
use serde_json::json;

use crate::RuntimeEvent;

use super::{AgentRuntime, RuntimeError};

const SUMMARY_PROMPT: &str = "\
Summarize the conversation so far into a concise handoff note.\n\
Include: key decisions, files modified, outstanding tasks, and current state.\n\
Output plain text only.";

const SUMMARY_MAX_OUTPUT_TOKENS: u32 = 2048;
const MIN_ENTRIES_FOR_COMPRESSION: usize = 4;

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn compress_context(&mut self) -> Result<(), RuntimeError> {
        let view = self.tape.default_view();
        if view.conversation.len() < MIN_ENTRIES_FOR_COMPRESSION {
            return Ok(());
        }
        let source_entry_ids = view.entries.iter().map(|entry| entry.id).collect::<Vec<_>>();

        let request = CompletionRequest {
            model: self.model_identity.clone(),
            instructions: Some(SUMMARY_PROMPT.to_string()),
            conversation: view.conversation,
            resume_checkpoint: None,
            max_output_tokens: Some(SUMMARY_MAX_OUTPUT_TOKENS),
            available_tools: Vec::new(),
        };

        let completion = self.model.complete(request).map_err(RuntimeError::model)?;
        let summary = completion.plain_text();

        self.tape.handoff(
            "context_compression",
            json!({
                "phase": "context_compression",
                "summary": summary,
                "next_steps": [],
                "source_entry_ids": source_entry_ids,
                "owner": "runtime"
            }),
        );

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
