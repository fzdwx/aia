use agent_prompts::SystemPromptConfig;

use super::build_session_system_prompt;

#[test]
fn default_session_prompt_keeps_context_contract() {
    let prompt = build_session_system_prompt(&SystemPromptConfig::default());

    assert!(prompt.contains("你是 aia 的助手"));
    assert!(prompt.contains("Context Contract"));
    assert!(prompt.contains("80%"));
    assert!(prompt.contains("95%"));
}

#[test]
fn custom_session_prompt_replaces_base_prompt_and_keeps_extensions() {
    let config = SystemPromptConfig::default()
        .with_custom_prompt("你是自定义客户端代理。")
        .with_guideline("优先输出 JSON")
        .with_append_section("附加客户端说明");

    let prompt = build_session_system_prompt(&config);

    assert!(prompt.contains("你是自定义客户端代理。"));
    assert!(!prompt.contains("你是 aia 的助手"));
    assert!(prompt.contains("优先输出 JSON"));
    assert!(prompt.contains("附加客户端说明"));
    assert!(prompt.contains("Context Contract"));
}
