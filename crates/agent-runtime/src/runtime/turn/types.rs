use agent_core::{CompletionUsage, StreamEvent};

use crate::{ToolInvocationLifecycle, TurnBlock};

pub(super) struct TurnBuffers {
    pub(super) source_entry_ids: Vec<u64>,
    pub(super) aggregated_thinking: String,
    pub(super) streamed_thinking: String,
    pub(super) tool_invocations: Vec<ToolInvocationLifecycle>,
    pub(super) blocks: Vec<TurnBlock>,
    pub(super) last_assistant_text: Option<String>,
    pub(super) streamed_assistant_text: String,
}

impl TurnBuffers {
    pub(super) fn new(user_entry_id: u64) -> Self {
        Self {
            source_entry_ids: vec![user_entry_id],
            aggregated_thinking: String::new(),
            streamed_thinking: String::new(),
            tool_invocations: Vec::new(),
            blocks: Vec::new(),
            last_assistant_text: None,
            streamed_assistant_text: String::new(),
        }
    }

    pub(super) fn thinking(&self) -> Option<String> {
        if self.aggregated_thinking.is_empty() {
            None
        } else {
            Some(self.aggregated_thinking.clone())
        }
    }

    pub(super) fn record_stream_event(&mut self, event: &StreamEvent) {
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

    pub(super) fn failure_context<'a>(
        &'a mut self,
        turn_id: &'a str,
        started_at_ms: u64,
        user_message: &'a str,
    ) -> TurnFailureContext<'a> {
        TurnFailureContext {
            turn_id,
            started_at_ms,
            user_message,
            source_entry_ids: &mut self.source_entry_ids,
            blocks: &self.blocks,
            assistant_message: self.last_assistant_text.clone(),
            aggregated_thinking: self.aggregated_thinking.as_str(),
            tool_invocations: &self.tool_invocations,
        }
    }

    pub(super) fn into_success_context(
        self,
        turn_id: String,
        started_at_ms: u64,
        user_message: String,
        usage: Option<CompletionUsage>,
    ) -> TurnSuccessContext {
        let thinking = self.thinking();

        TurnSuccessContext {
            turn_id,
            started_at_ms,
            source_entry_ids: self.source_entry_ids,
            user_message,
            blocks: self.blocks,
            tool_invocations: self.tool_invocations,
            summary: TurnCompletionSummary {
                assistant_message: self.last_assistant_text,
                thinking,
                usage,
            },
        }
    }
}

pub(crate) struct TurnCompletionSummary {
    pub(crate) assistant_message: Option<String>,
    pub(crate) thinking: Option<String>,
    pub(crate) usage: Option<CompletionUsage>,
}

pub(crate) struct TurnSuccessContext {
    pub(crate) turn_id: String,
    pub(crate) started_at_ms: u64,
    pub(crate) source_entry_ids: Vec<u64>,
    pub(crate) user_message: String,
    pub(crate) blocks: Vec<TurnBlock>,
    pub(crate) tool_invocations: Vec<ToolInvocationLifecycle>,
    pub(crate) summary: TurnCompletionSummary,
}

pub(crate) struct TurnFailureContext<'a> {
    pub(crate) turn_id: &'a str,
    pub(crate) started_at_ms: u64,
    pub(crate) user_message: &'a str,
    pub(crate) source_entry_ids: &'a mut Vec<u64>,
    pub(crate) blocks: &'a [TurnBlock],
    pub(crate) assistant_message: Option<String>,
    pub(crate) aggregated_thinking: &'a str,
    pub(crate) tool_invocations: &'a [ToolInvocationLifecycle],
}
