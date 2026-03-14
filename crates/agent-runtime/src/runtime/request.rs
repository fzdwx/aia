use agent_core::{
    CompletionRequest, ConversationItem, LanguageModel, LlmTraceRequestContext, ToolDefinition,
    ToolExecutor,
};

use crate::ContextStats;

use super::{AgentRuntime, helpers::anchor_state_message};

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
            conversation.push(ConversationItem::Message(anchor_state_message(anchor)));
        }
        conversation.extend(view.conversation);

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
            trace_context: Some(LlmTraceRequestContext {
                trace_id: format!("{turn_id}-{request_kind}-{step_index}"),
                turn_id: turn_id.to_string(),
                run_id: turn_id.to_string(),
                request_kind: request_kind.to_string(),
                step_index,
            }),
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

        let available_tools = self.visible_tools();
        let instructions = self.instructions.as_ref().filter(|text| !text.is_empty()).cloned();
        let estimated_context_units = Self::approximate_request_units(
            instructions.as_deref(),
            &view.conversation,
            &available_tools,
        );

        let context_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.context);
        let output_limit = self.model_identity.limit.as_ref().and_then(|limit| limit.output);
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
        let view = self.tape.default_view();
        let available_tools = self.visible_tools();
        let instructions = self.instructions.as_ref().filter(|text| !text.is_empty()).cloned();
        let estimated = Self::approximate_request_units(
            instructions.as_deref(),
            &view.conversation,
            &available_tools,
        );
        Some(estimated as f64 / context_limit as f64)
    }
}
