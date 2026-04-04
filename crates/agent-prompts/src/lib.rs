mod system_prompt;
mod title_generator;
pub mod tool_descriptions;
pub mod widget_guidelines;

pub use system_prompt::{SystemPromptBlock, SystemPromptConfig, build_system_prompt};
pub use title_generator::{
    TitleGeneratorPromptContext, render_title_generator_prompt, title_generator_prompt_template,
};

const AIA_AGENTS_TEMPLATE: &str = include_str!("../prompts/aia-agents.md");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiaAgentsPromptContext {
    pub platform: String,
    pub working_directory: String,
    pub local_date: String,
    pub weekday: String,
    pub timezone: String,
}

/// Auto-compression: structured handoff summary prompt template.
///
/// Contains `{{token_budget}}` — call [`handoff_summary`] to render.
const HANDOFF_SUMMARY_TEMPLATE: &str = include_str!("../prompts/handoff-summary.md");

/// Context contract template injected into system instructions.
///
/// Contains `{{agent_handoff_threshold}}` and `{{auto_compression_threshold}}`
/// placeholders — call [`context_contract`] to render.
const CONTEXT_CONTRACT_TEMPLATE: &str = include_str!("../prompts/context-contract.md");

/// Recommended threshold for the agent to proactively call TapeHandoff.
pub const AGENT_HANDOFF_THRESHOLD: f64 = 0.80;

/// Threshold at which the runtime auto-compresses context.
pub const AUTO_COMPRESSION_THRESHOLD: f64 = 0.90;

pub fn aia_agents_prompt_template() -> &'static str {
    AIA_AGENTS_TEMPLATE.trim()
}

pub fn render_aia_agents_prompt(context: AiaAgentsPromptContext) -> String {
    render(
        aia_agents_prompt_template(),
        &[
            ("platform", &context.platform),
            ("working_directory", &context.working_directory),
            ("local_date", &context.local_date),
            ("weekday", &context.weekday),
            ("timezone", &context.timezone),
        ],
    )
}

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
#[path = "../tests/lib/mod.rs"]
mod tests;
