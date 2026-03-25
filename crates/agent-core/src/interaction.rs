use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionInteractionCapabilities {
    #[serde(default)]
    pub supports_interactive_components: bool,
    #[serde(default)]
    pub supports_question_tool: bool,
}

impl SessionInteractionCapabilities {
    pub fn interactive() -> Self {
        Self { supports_interactive_components: true, supports_question_tool: true }
    }

    pub fn non_interactive() -> Self {
        Self { supports_interactive_components: false, supports_question_tool: false }
    }

    pub fn can_use_question_tool(&self) -> bool {
        self.supports_interactive_components && self.supports_question_tool
    }
}
