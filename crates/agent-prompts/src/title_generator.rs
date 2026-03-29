const TITLE_GENERATOR_TEMPLATE: &str = include_str!("../prompts/title-generator.md");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleGeneratorPromptContext {
    pub current_title: String,
    pub title_source: String,
    pub recent_user_turns: Vec<String>,
}

pub fn title_generator_prompt_template() -> &'static str {
    TITLE_GENERATOR_TEMPLATE.trim()
}

pub fn render_title_generator_prompt(context: TitleGeneratorPromptContext) -> String {
    let conversation_excerpt = build_title_generator_conversation_excerpt(&context);
    super::render(
        title_generator_prompt_template(),
        &[("conversation_excerpt", &conversation_excerpt)],
    )
}

fn build_title_generator_conversation_excerpt(context: &TitleGeneratorPromptContext) -> String {
    let mut excerpt = format!(
        "Current title: {}\nTitle source: {}\nRecent user messages:\n",
        context.current_title, context.title_source,
    );

    for (index, turn) in context.recent_user_turns.iter().enumerate() {
        excerpt.push_str(format!("{}. {}\n", index + 1, turn).as_str());
    }

    excerpt
}
