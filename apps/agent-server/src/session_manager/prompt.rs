use agent_prompts::{SystemPromptBlock, SystemPromptConfig, build_system_prompt};

const DEFAULT_SESSION_SYSTEM_PROMPT: &str = "你是 aia 的助手。给出清晰、结构化的答案。";

pub(super) fn build_session_system_prompt(config: &SystemPromptConfig) -> String {
    let config = config.clone().with_context_block(SystemPromptBlock::new(
        "Context Contract",
        agent_prompts::context_contract(
            agent_prompts::AGENT_HANDOFF_THRESHOLD,
            agent_prompts::AUTO_COMPRESSION_THRESHOLD,
        ),
    ));
    build_system_prompt(DEFAULT_SESSION_SYSTEM_PROMPT, &config)
}

#[cfg(test)]
#[path = "../../tests/session_manager/prompt/mod.rs"]
mod tests;
