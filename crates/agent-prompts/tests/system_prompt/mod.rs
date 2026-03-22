use super::{SystemPromptBlock, SystemPromptConfig, build_system_prompt};

#[test]
fn build_system_prompt_uses_explicit_base_prompt() {
    let config = SystemPromptConfig::default();

    let prompt = build_system_prompt("你是一个测试助手。", &config);

    assert_eq!(prompt, "你是一个测试助手。");
}

#[test]
fn builder_appends_guidelines_sections_and_context_blocks() {
    let config = SystemPromptConfig::default()
        .with_guideline("保持简洁")
        .with_guideline("显示文件路径")
        .with_append_section("额外说明")
        .with_context_block(SystemPromptBlock::new("Project Context", "当前项目要求"));

    let prompt = build_system_prompt("默认提示", &config);

    assert!(prompt.contains("默认提示"));
    assert!(prompt.contains("Additional guidelines:\n- 保持简洁\n- 显示文件路径"));
    assert!(prompt.contains("额外说明"));
    assert!(prompt.contains("# Project Context\n\n当前项目要求"));
}

#[test]
fn builder_skips_empty_values_and_deduplicates_text_sections() {
    let config = SystemPromptConfig {
        guidelines: vec!["  ".into(), "保持简洁".into(), "保持简洁".into()],
        append_sections: vec!["".into(), "附加说明".into(), "附加说明".into()],
        context_blocks: vec![
            SystemPromptBlock::new(" ", "ignored"),
            SystemPromptBlock::new("Valid", "context"),
        ],
    };

    let prompt = build_system_prompt("默认提示", &config);

    assert_eq!(prompt.matches("保持简洁").count(), 1);
    assert_eq!(prompt.matches("附加说明").count(), 1);
    assert!(prompt.contains("# Valid\n\ncontext"));
    assert!(!prompt.contains("ignored"));
}

#[test]
fn build_system_prompt_keeps_additions_on_top_of_explicit_base_prompt() {
    let config = SystemPromptConfig::default()
        .with_guideline("保持简洁")
        .with_append_section("附加说明")
        .with_context_block(SystemPromptBlock::new("Project Context", "当前项目要求"));

    let prompt = build_system_prompt("你是一个测试助手。", &config);

    assert!(prompt.contains("你是一个测试助手。"));
    assert!(prompt.contains("Additional guidelines:\n- 保持简洁"));
    assert!(prompt.contains("附加说明"));
    assert!(prompt.contains("# Project Context\n\n当前项目要求"));
}

#[test]
fn aia_agents_prompt_renders_runtime_environment_values() {
    let prompt = crate::render_aia_agents_prompt(crate::AiaAgentsPromptContext {
        platform: "linux".into(),
        working_directory: "/tmp/runtime-workspace".into(),
        local_date: "2030-01-02".into(),
        weekday: "Thursday".into(),
        timezone: "UTC+9".into(),
    });

    assert!(prompt.contains("SYSTEM INFO - You are running on linux."));
    assert!(prompt.contains(
        "WORKING DIRECTORY - Your current working directory is: /tmp/runtime-workspace."
    ));
    assert!(prompt.contains("Authoritative local date: 2030-01-02"));
    assert!(prompt.contains("Weekday: Thursday"));
    assert!(prompt.contains("Timezone: UTC+9"));
    assert!(!prompt.contains("/home/like/.config/Aia/workspaces/temp-mn115sevfdrz553q51u"));
    assert!(!prompt.contains("2026-03-22"));
}

#[test]
fn aia_agents_prompt_template_matches_embedded_template() {
    let prompt = crate::aia_agents_prompt_template();

    assert_eq!(prompt, include_str!("../../prompts/aia-agents.md").trim());
    assert!(prompt.contains("You are Aia — not an assistant, not a chatbot, just... Aia."));
    assert!(prompt.contains("{{working_directory}}"));
}
