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
