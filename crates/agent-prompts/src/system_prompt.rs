use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemPromptBlock {
    pub title: String,
    pub content: String,
}

impl SystemPromptBlock {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self { title: title.into(), content: content.into() }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemPromptConfig {
    pub guidelines: Vec<String>,
    pub append_sections: Vec<String>,
    pub context_blocks: Vec<SystemPromptBlock>,
}

impl SystemPromptConfig {
    pub fn with_guideline(mut self, guideline: impl Into<String>) -> Self {
        self.guidelines.push(guideline.into());
        self
    }

    pub fn with_append_section(mut self, section: impl Into<String>) -> Self {
        self.append_sections.push(section.into());
        self
    }

    pub fn with_context_block(mut self, block: SystemPromptBlock) -> Self {
        self.context_blocks.push(block);
        self
    }
}

pub fn build_system_prompt(base_prompt: impl AsRef<str>, config: &SystemPromptConfig) -> String {
    let mut sections = Vec::new();
    let prompt = base_prompt.as_ref().trim();
    if !prompt.is_empty() {
        sections.push(prompt.to_string());
    }

    let guidelines = dedupe_non_empty(&config.guidelines);
    if !guidelines.is_empty() {
        sections.push(format!(
            "Additional guidelines:\n{}",
            guidelines
                .iter()
                .map(|guideline| format!("- {guideline}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    for section in dedupe_non_empty(&config.append_sections) {
        sections.push(section);
    }

    for block in config.context_blocks.iter().filter(|block| !block.is_empty()) {
        sections.push(block.to_string());
    }

    sections.join("\n\n")
}

impl SystemPromptBlock {
    fn is_empty(&self) -> bool {
        self.title.trim().is_empty() || self.content.trim().is_empty()
    }
}

impl fmt::Display for SystemPromptBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "# {}\n\n{}", self.title.trim(), self.content.trim())
    }
}

fn dedupe_non_empty(values: &[String]) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || deduped.iter().any(|existing| existing == trimmed) {
            continue;
        }
        deduped.push(trimmed.to_string());
    }
    deduped
}

#[cfg(test)]
#[path = "../tests/system_prompt/mod.rs"]
mod tests;
