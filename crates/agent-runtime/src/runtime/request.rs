use agent_core::{
    CompletionRequest, ConversationItem, LanguageModel, ToolDefinition, ToolExecutor,
};

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
    pub(super) fn build_completion_request(
        &self,
        turn_id: &str,
        request_kind: &str,
        step_index: u32,
    ) -> CompletionRequest {
        let view = self.tape.default_view();
        let mut conversation = Vec::new();
        if let Some(anchor) = view.origin_anchor.as_ref() {
            if let Some(msg) = anchor_state_message(anchor) {
                conversation.push(ConversationItem::Message(msg));
            }
        }
        conversation.extend(drop_orphaned_tool_results(view.conversation));

        let available_tools = self.visible_tools();
        let instructions = self.instructions.as_ref().filter(|text| !text.is_empty()).cloned();
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
            trace_context: Some(build_llm_trace_context(
                turn_id,
                turn_id,
                request_kind,
                step_index,
            )),
        }
    }

    fn effective_max_output_tokens(
        &self,
        instructions: Option<&str>,
        conversation: &[ConversationItem],
        available_tools: &[ToolDefinition],
    ) -> Option<u32> {
        let configured_output = self.model_identity.limit.as_ref().and_then(|limit| limit.output);
        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context);

        let Some(context_limit) = context_limit else {
            return configured_output;
        };

        let estimated_usage =
            Self::approximate_request_units(instructions, conversation, available_tools);
        let usable_headroom = context_limit
            .saturating_sub(estimated_usage.saturating_add(CONTEXT_HEADROOM_SAFETY_TOKENS));
        let effective =
            configured_output.map_or(usable_headroom, |output| output.min(usable_headroom));

        Some(effective.max(1))
    }

    fn approximate_request_units(
        instructions: Option<&str>,
        conversation: &[ConversationItem],
        available_tools: &[ToolDefinition],
    ) -> u32 {
        let instruction_units = instructions.map_or(0, Self::approximate_text_units);
        let conversation_units = conversation
            .iter()
            .map(|item| match item {
                ConversationItem::Message(message) => {
                    Self::approximate_text_units(&message.content)
                }
                ConversationItem::ToolCall(call) => Self::approximate_text_units(&call.tool_name)
                    .saturating_add(Self::approximate_text_units(&call.arguments.to_string())),
                ConversationItem::ToolResult(result) => {
                    Self::approximate_text_units(&result.tool_name)
                        .saturating_add(Self::approximate_text_units(&result.content))
                }
            })
            .sum::<u32>();
        let tool_units = available_tools
            .iter()
            .map(|tool| {
                Self::approximate_text_units(&tool.name)
                    .saturating_add(Self::approximate_text_units(&tool.description))
                    .saturating_add(Self::approximate_text_units(&tool.parameters.to_string()))
            })
            .sum::<u32>();

        instruction_units.saturating_add(conversation_units).saturating_add(tool_units)
    }

    fn approximate_text_units(text: &str) -> u32 {
        text.chars().count().min(u32::MAX as usize) as u32
    }

    pub fn context_stats(&self) -> ContextStats {
        let view = self.tape.default_view();
        let anchors = self.tape.anchors();
        let anchor_count = anchors.len();
        let entries_since_last_anchor = view.entries.len();
        let total_entries = self.tape.entries().len();

        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context);
        let output_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.output);

        // Display: use real token count from last model call, 0 if no call yet.
        let estimated_context_units =
            self.last_input_tokens.unwrap_or(0).min(u32::MAX as u64) as u32;
        let pressure_ratio =
            context_limit.map(|limit| estimated_context_units as f64 / limit as f64);

        ContextStats {
            total_entries,
            anchor_count,
            entries_since_last_anchor,
            estimated_context_units,
            context_limit,
            output_limit,
            pressure_ratio,
        }
    }

    pub(super) fn context_pressure_ratio(&self) -> Option<f64> {
        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context)?;
        let input_tokens = self.last_input_tokens.unwrap_or(0);
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
                .map_or(true, |result| known_call_ids.contains(&result.invocation_id))
        })
        .collect()
}
