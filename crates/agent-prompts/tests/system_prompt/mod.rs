use super::{SystemPromptBlock, SystemPromptConfig, build_system_prompt};

#[test]
fn custom_prompt_replaces_base_prompt() {
    let config = SystemPromptConfig::default().with_custom_prompt("你是一个测试助手。");

    let prompt = build_system_prompt("默认提示", &config);

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
        custom_prompt: None,
        prompt_guidelines: vec!["  ".into(), "保持简洁".into(), "保持简洁".into()],
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
