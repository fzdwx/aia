use serde_json::json;
use session_tape::TapeEntry;

use agent_core::{LanguageModel, ToolExecutor};

use crate::{TurnBlock, TurnLifecycle, TurnOutcome};

use super::{
    AgentRuntime, RuntimeError,
    helpers::now_timestamp_ms,
    turn::{TurnFailureContext, TurnSuccessContext},
};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn finish_success_turn(
        &mut self,
        context: TurnSuccessContext,
    ) -> Result<(), RuntimeError> {
        let completion_event_id = self.append_tape_entry(
            TapeEntry::event(
                "turn_completed",
                Some(json!({"status": "ok", "usage": context.summary.usage.clone()})),
            )
            .with_run_id(&context.turn_id)
            .with_meta("source_entry_ids", json!(context.source_entry_ids.clone())),
        )?;
        let mut source_entry_ids = context.source_entry_ids;
        source_entry_ids.push(completion_event_id);
        self.publish_turn_lifecycle(TurnLifecycle {
            turn_id: context.turn_id,
            started_at_ms: context.started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            source_entry_ids,
            user_message: context.user_message,
            blocks: context.blocks,
            assistant_message: context.summary.assistant_message,
            thinking: context.summary.thinking,
            tool_invocations: context.tool_invocations,
            usage: context.summary.usage,
            failure_message: None,
            outcome: TurnOutcome::Succeeded,
        });
        Ok(())
    }

    pub(super) fn record_turn_failure(
        &mut self,
        context: TurnFailureContext<'_>,
        runtime_error: RuntimeError,
    ) -> Result<(), RuntimeError> {
        let failure_event_id = self.append_tape_entry(
            TapeEntry::event("turn_failed", Some(json!({"message": runtime_error.to_string()})))
                .with_run_id(context.turn_id)
                .with_meta("source_entry_ids", json!(context.source_entry_ids.clone())),
        )?;
        context.source_entry_ids.push(failure_event_id);
        self.publish_event(crate::RuntimeEvent::TurnFailed { message: runtime_error.to_string() });
        let mut lifecycle_blocks = context.blocks.to_vec();
        lifecycle_blocks.push(TurnBlock::Failure { message: runtime_error.to_string() });
        self.publish_turn_lifecycle(TurnLifecycle {
            turn_id: context.turn_id.to_string(),
            started_at_ms: context.started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            source_entry_ids: context.source_entry_ids.clone(),
            user_message: context.user_message.to_string(),
            blocks: lifecycle_blocks,
            assistant_message: context.assistant_message,
            thinking: if context.aggregated_thinking.is_empty() {
                None
            } else {
                Some(context.aggregated_thinking.to_string())
            },
            tool_invocations: context.tool_invocations.to_vec(),
            usage: None,
            failure_message: Some(runtime_error.to_string()),
            outcome: if runtime_error.is_cancelled() {
                TurnOutcome::Cancelled
            } else {
                TurnOutcome::Failed
            },
        });
        Ok(())
    }
}
