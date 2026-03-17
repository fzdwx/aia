pub mod tool_descriptions;

/// Auto-compression: structured handoff summary prompt template.
///
/// Contains `{{token_budget}}` — call [`handoff_summary`] to render.
const HANDOFF_SUMMARY_TEMPLATE: &str = include_str!("../prompts/handoff-summary.md");

/// Context contract template injected into system instructions.
///
/// Contains `{{agent_handoff_threshold}}` and `{{auto_compression_threshold}}`
/// placeholders — call [`context_contract`] to render.
const CONTEXT_CONTRACT_TEMPLATE: &str = include_str!("../prompts/context-contract.md");

/// Recommended threshold for the agent to proactively call tape_handoff.
pub const AGENT_HANDOFF_THRESHOLD: f64 = 0.80;

/// Threshold at which the runtime auto-compresses context.
pub const AUTO_COMPRESSION_THRESHOLD: f64 = 0.95;

/// Render the handoff summary prompt with the given token budget.
pub fn handoff_summary(token_budget: u32) -> String {
    render(HANDOFF_SUMMARY_TEMPLATE, &[("token_budget", &token_budget.to_string())])
}

/// Render the context contract block with the given thresholds.
pub fn context_contract(agent_handoff_threshold: f64, auto_compression_threshold: f64) -> String {
    render(
        CONTEXT_CONTRACT_TEMPLATE,
        &[
            ("agent_handoff_threshold", &format_percent(agent_handoff_threshold)),
            ("auto_compression_threshold", &format_percent(auto_compression_threshold)),
        ],
    )
}

/// Replace `{{key}}` placeholders in `template` with corresponding values.
///
/// Panics (debug) if any `{{…}}` placeholder remains after substitution.
fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }
    debug_assert!(!result.contains("{{"), "unresolved placeholder in rendered template: {result}");
    result
}

fn format_percent(value: f64) -> String {
    format!("{}%", (value * 100.0) as u32)
}

#[cfg(test)]
mod tests {
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
        let result =
            render("Hello {{name}}, you are {{age}}.", &[("name", "Alice"), ("age", "30")]);
        assert_eq!(result, "Hello Alice, you are 30.");
    }

    #[test]
    #[should_panic(expected = "unresolved placeholder")]
    fn render_panics_on_missing_var_in_debug() {
        render("Hello {{name}}", &[]);
    }
}
