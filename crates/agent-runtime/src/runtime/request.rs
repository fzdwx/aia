use agent_core::{CompletionRequest, ConversationItem, LanguageModel, ToolExecutor};

use super::{AgentRuntime, helpers::anchor_state_message};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn build_completion_request(
        &self,
        current_step: usize,
        force_text_only: bool,
    ) -> CompletionRequest {
        let view = self.tape.default_view();
        let checkpoint = self.tape.latest_model_checkpoint();
        let should_resume_from_checkpoint = self
            .tape
            .latest_provider_binding()
            .and_then(|binding| match binding {
                session_tape::SessionProviderBinding::Provider { protocol, .. } => Some(protocol),
                session_tape::SessionProviderBinding::Bootstrap => None,
            })
            .is_some_and(|protocol| {
                protocol == "openai-responses"
                    && checkpoint.as_ref().is_some_and(|checkpoint| {
                        checkpoint.checkpoint.protocol == "openai-responses"
                    })
            });
        let conversation = if should_resume_from_checkpoint {
            self.tape.conversation_since(
                checkpoint.as_ref().map(|checkpoint| checkpoint.checkpoint_entry_id).unwrap_or(0),
            )
        } else {
            let mut conversation = Vec::new();
            if let Some(anchor) = view.origin_anchor.as_ref() {
                conversation.push(ConversationItem::Message(anchor_state_message(anchor)));
            }
            conversation.extend(view.conversation);
            conversation
        };

        let remaining_steps = self.max_turn_steps.saturating_sub(current_step);
        let budget_hint = if force_text_only {
            format!(
                "{} 当前为第 {current_step}/{max} 步，剩余可调用工具步数为 0。{}",
                Self::STEP_BUDGET_INSTRUCTION_PREFIX,
                Self::FINAL_TEXT_ONLY_INSTRUCTION,
                max = self.max_turn_steps,
            )
        } else {
            format!(
                "{} 当前为第 {current_step}/{max} 步，剩余可继续调用工具的步数为 {remaining_steps}。如果信息已经足够，请尽早直接给出最终回答。",
                Self::STEP_BUDGET_INSTRUCTION_PREFIX,
                max = self.max_turn_steps,
            )
        };
        let instructions = match self.instructions.as_ref() {
            Some(instructions) if !instructions.is_empty() => {
                Some(format!("{instructions}\n\n{budget_hint}"))
            }
            _ => Some(budget_hint),
        };

        CompletionRequest {
            model: self.model_identity.clone(),
            instructions,
            conversation,
            resume_checkpoint: if should_resume_from_checkpoint {
                checkpoint.map(|value| value.checkpoint)
            } else {
                None
            },
            available_tools: if force_text_only { vec![] } else { self.visible_tools() },
        }
    }
}
