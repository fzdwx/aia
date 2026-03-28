use super::*;

#[test]
fn handoff_summary_renders_token_budget() {
    let prompt = handoff_summary(8192);
    assert!(prompt.contains("handoff summary"));
    assert!(prompt.contains("8192"));
    assert!(!prompt.contains("{{"));
}

#[test]
fn context_contract_renders_both_thresholds() {
    let rendered = context_contract(0.80, 0.95);
    assert!(rendered.contains("80%"));
    assert!(rendered.contains("95%"));
    assert!(!rendered.contains("{{"));
}

#[test]
fn render_replaces_all_placeholders() {
    let result = render("Hello {{name}}, you are {{age}}.", &[("name", "Alice"), ("age", "30")]);
    assert_eq!(result, "Hello Alice, you are 30.");
}

#[test]
#[should_panic(expected = "unresolved placeholder")]
fn render_panics_on_missing_var_in_debug() {
    render("Hello {{name}}", &[]);
}

#[test]
fn title_generator_prompt_renders_structured_context() {
    let prompt = crate::render_title_generator_prompt(crate::TitleGeneratorPromptContext {
        current_title: "New session".into(),
        title_source: "default".into(),
        recent_user_turns: vec![
            "debug 500 errors in production".into(),
            "why is app.js failing".into(),
        ],
    });

    assert!(prompt.contains("You are a title generator."));
    assert!(prompt.contains("Current title: New session"));
    assert!(prompt.contains("Title source: default"));
    assert!(prompt.contains("1. debug 500 errors in production"));
    assert!(prompt.contains("2. why is app.js failing"));
    assert!(!prompt.contains("{{"));
}

#[test]
fn title_generator_prompt_template_matches_embedded_template() {
    let prompt = crate::title_generator_prompt_template();

    assert_eq!(prompt, include_str!("../../prompts/title-generator.md").trim());
    assert!(prompt.contains("Generate a brief title that would help the user find this conversation later."));
    assert!(prompt.contains("{{conversation_excerpt}}"));
}
