use serde_json::json;
use session_tape::TapeEntry;

use agent_core::{LanguageModel, ToolExecutor};

use crate::{ToolInvocationLifecycle, TurnBlock, TurnLifecycle};

use super::{AgentRuntime, RuntimeError, helpers::now_timestamp_ms};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn finish_success_turn(
        &mut self,
        turn_id: String,
        started_at_ms: u64,
        source_entry_ids: Vec<u64>,
        user_message: String,
        blocks: Vec<TurnBlock>,
        assistant_message: Option<String>,
        thinking: Option<String>,
        tool_invocations: Vec<ToolInvocationLifecycle>,
    ) -> Result<(), RuntimeError> {
        let completion_event_id = self.append_tape_entry(
            TapeEntry::event("turn_completed", Some(json!({"status": "ok"})))
                .with_run_id(&turn_id)
                .with_meta("source_entry_ids", json!(source_entry_ids.clone())),
        )?;
        let mut source_entry_ids = source_entry_ids;
        source_entry_ids.push(completion_event_id);
        self.publish_turn_lifecycle(TurnLifecycle {
            turn_id,
            started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            source_entry_ids,
            user_message,
            blocks,
            assistant_message,
            thinking,
            tool_invocations,
            failure_message: None,
        });
        Ok(())
    }

    pub(super) fn record_turn_failure(
        &mut self,
        turn_id: &str,
        started_at_ms: u64,
        source_entry_ids: &mut Vec<u64>,
        user_message: &str,
        blocks: &[TurnBlock],
        assistant_message: Option<String>,
        aggregated_thinking: &str,
        tool_invocations: &[ToolInvocationLifecycle],
        runtime_error: RuntimeError,
    ) -> Result<(), RuntimeError> {
        let failure_event_id = self.append_tape_entry(
            TapeEntry::event("turn_failed", Some(json!({"message": runtime_error.to_string()})))
                .with_run_id(turn_id)
                .with_meta("source_entry_ids", json!(source_entry_ids.clone())),
        )?;
        source_entry_ids.push(failure_event_id);
        self.publish_event(crate::RuntimeEvent::TurnFailed { message: runtime_error.to_string() });
        let mut lifecycle_blocks = blocks.to_vec();
        lifecycle_blocks.push(TurnBlock::Failure { message: runtime_error.to_string() });
        self.publish_turn_lifecycle(TurnLifecycle {
            turn_id: turn_id.to_string(),
            started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            source_entry_ids: source_entry_ids.clone(),
            user_message: user_message.to_string(),
            blocks: lifecycle_blocks,
            assistant_message,
            thinking: if aggregated_thinking.is_empty() {
                None
            } else {
                Some(aggregated_thinking.to_string())
            },
            tool_invocations: tool_invocations.to_vec(),
            failure_message: Some(runtime_error.to_string()),
        });
        Ok(())
    }
}
