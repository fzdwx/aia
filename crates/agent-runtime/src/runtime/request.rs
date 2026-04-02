use agent_core::{CompletionRequest, ConversationItem, LanguageModel, ToolDefinition, ToolExecutor};

use crate::ContextStats;

use super::{
    AgentRuntime,
    helpers::{anchor_state_message, build_llm_trace_context},
};

const CONTEXT_HEADROOM_SAFETY_TOKENS: u32 = 32;
impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn should_preflight_compress(&self, turn_id: &str) -> bool {
        if self.current_turn_has_tool_result_since_usage(turn_id) {
            return true;
        }

        let Some(context_limit) = self.model_identity.limit.as_ref().and_then(|limit| limit.context)
        else {
            return false;
        };
        let Some(last_input_tokens) = self.current_input_tokens() else {
            return false;
        };

        let pressure_ratio = last_input_tokens as f64 / context_limit as f64;
        if pressure_ratio < self.context_pressure_threshold {
            return false;
        }

        self.has_new_context_since_last_usage_sample(turn_id)
    }

    pub(super) fn build_completion_request(
        &self,
        turn_id: &str,
        request_kind: &str,
        step_index: u32,
    ) -> CompletionRequest {
        let (instructions, conversation, available_tools) = self.default_request_parts(turn_id);
        let max_output_tokens = self.effective_max_output_tokens(
            instructions.as_deref(),
            &conversation,
            &available_tools,
        );

        CompletionRequest {
            model: self.model_identity.clone(),
            instructions,
            conversation,
            max_output_tokens,
            available_tools,
            parallel_tool_calls: Some(true),
            prompt_cache: self.prompt_cache.clone(),
            user_agent: self.user_agent.clone(),
            timeout: self.request_timeout.clone(),
            trace_context: Some(build_llm_trace_context(
                self.session_id.as_deref(),
                turn_id,
                turn_id,
                request_kind,
                step_index,
            )),
        }
    }

    fn default_request_parts(
        &self,
        turn_id: &str,
    ) -> (Option<String>, Vec<ConversationItem>, Vec<ToolDefinition>) {
        let view = self.tape.default_view();
        let mut conversation = Vec::new();
        if let Some(anchor) = view.origin_anchor.as_ref()
            && let Some(msg) = anchor_state_message(anchor)
        {
            conversation.push(ConversationItem::Message(msg));
            conversation
                .extend(self.pending_turn_conversation_before_anchor(turn_id, anchor.entry_id));
        }
        conversation.extend(view.conversation);
        conversation = drop_orphaned_tool_results(conversation);

        let available_tools = self.visible_tools();
        let instructions = self.instructions.as_ref().filter(|text| !text.is_empty()).cloned();
        (instructions, conversation, available_tools)
    }

    fn current_input_tokens(&self) -> Option<u32> {
        if self.has_anchor_after_last_completed_turn() {
            return None;
        }

        self.last_input_tokens.map(|value| value.min(u32::MAX as u64) as u32)
    }

    fn has_new_context_since_last_usage_sample(&self, turn_id: &str) -> bool {
        if self.last_usage_turn_id.as_deref() != Some(turn_id) {
            return false;
        }

        let lower_bound = self.last_usage_entry_id.unwrap_or(0);
        self.tape.entries().iter().any(|entry| {
            let is_current_turn_entry =
                entry.meta.get("run_id").and_then(|value| value.as_str()) == Some(turn_id);
            entry.id > lower_bound
                && is_current_turn_entry
                && (entry.as_message().is_some()
                    || entry.as_tool_call().is_some()
                    || entry.as_tool_result().is_some())
        })
    }

    fn current_turn_has_tool_result_since_usage(&self, turn_id: &str) -> bool {
        if self.last_usage_turn_id.as_deref() != Some(turn_id) {
            return false;
        }

        let lower_bound = self.last_usage_entry_id.unwrap_or(0);
        self.tape.entries().iter().any(|entry| {
            entry.id > lower_bound
                && entry.meta.get("run_id").and_then(|value| value.as_str()) == Some(turn_id)
                && entry.as_tool_result().is_some()
        })
    }

    fn has_anchor_after_last_completed_turn(&self) -> bool {
        let Some(last_completed_turn_index) = self.latest_completed_turn_index() else {
            return false;
        };
        self.tape
            .entries()
            .iter()
            .skip(last_completed_turn_index.saturating_add(1))
            .any(|entry| entry.anchor_name().is_some())
    }

    fn latest_completed_turn_index(&self) -> Option<usize> {
        self.tape.entries().iter().enumerate().rev().find_map(|(index, entry)| {
            (entry.event_name() == Some("turn_completed")).then_some(index)
        })
    }

    fn pending_turn_conversation_before_anchor(
        &self,
        turn_id: &str,
        anchor_entry_id: u64,
    ) -> Vec<ConversationItem> {
        self.tape
            .entries()
            .iter()
            .filter(|entry| entry.id < anchor_entry_id)
            .filter(|entry| {
                entry.meta.get("run_id").and_then(|value| value.as_str()) == Some(turn_id)
            })
            .filter_map(|entry| {
                entry
                    .as_message()
                    .map(ConversationItem::Message)
                    .or_else(|| entry.as_tool_call().map(ConversationItem::ToolCall))
                    .or_else(|| entry.as_tool_result().map(ConversationItem::ToolResult))
            })
            .collect()
    }

    fn effective_max_output_tokens(
        &self,
        _instructions: Option<&str>,
        _conversation: &[agent_core::ConversationItem],
        _available_tools: &[ToolDefinition],
    ) -> Option<u32> {
        let configured_output = self.model_identity.limit.as_ref().and_then(|limit| limit.output);
        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context);

        let Some(context_limit) = context_limit else {
            return configured_output;
        };
        let Some(last_input_tokens) = self.current_input_tokens() else {
            return configured_output;
        };

        let usable_headroom = context_limit
            .saturating_sub(last_input_tokens.saturating_add(CONTEXT_HEADROOM_SAFETY_TOKENS));
        let effective =
            configured_output.map_or(usable_headroom, |output| output.min(usable_headroom));

        Some(effective.max(1))
    }

    pub fn context_stats(&self) -> ContextStats {
        let view = self.tape.default_view();
        let anchors = self.tape.anchors();
        let anchor_count = anchors.len();
        let entries_since_last_anchor = view.entries.len();
        let total_entries = self.tape.entries().len();

        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context);
        let output_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.output);

        let last_input_tokens = self.current_input_tokens();
        let pressure_ratio = match (last_input_tokens, context_limit) {
            (Some(input_tokens), Some(limit)) => Some(input_tokens as f64 / limit as f64),
            _ => None,
        };

        ContextStats {
            total_entries,
            anchor_count,
            entries_since_last_anchor,
            last_input_tokens,
            context_limit,
            output_limit,
            pressure_ratio,
        }
    }

    pub(super) fn context_pressure_ratio(&self) -> Option<f64> {
        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context)?;
        let input_tokens = self.current_input_tokens()?;
        Some(input_tokens as f64 / context_limit as f64)
    }
}

/// Drop ToolResult items whose matching ToolCall was truncated by an anchor.
///
/// After a `tape_handoff`, the anchor sits between the tool_call and tool_result entries.
/// The view only includes entries after the anchor, so the tool_call is gone but the
/// tool_result remains — an "orphan". Sending an orphaned ToolResult to the model without
/// a preceding ToolCall causes API errors. This function filters them out.
pub(super) fn drop_orphaned_tool_results(
    conversation: Vec<ConversationItem>,
) -> Vec<ConversationItem> {
    use std::collections::BTreeSet;

    let known_call_ids: BTreeSet<String> = conversation
        .iter()
        .filter_map(|item| item.as_tool_call().map(|call| call.invocation_id.clone()))
        .collect();

    conversation
        .into_iter()
        .filter(|item| {
            item.as_tool_result()
                .is_none_or(|result| known_call_ids.contains(&result.invocation_id))
        })
        .collect()
}
