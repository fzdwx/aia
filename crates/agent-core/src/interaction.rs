use serde::{Deserialize, Serialize};

pub const QUESTION_INTERACTION_KIND: &str = "question";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionInteractionCapabilities {
    #[serde(default)]
    pub supports_interactive_components: bool,
    #[serde(default)]
    pub supports_question_tool: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_interaction_kinds: Vec<String>,
}

impl SessionInteractionCapabilities {
    pub fn interactive() -> Self {
        Self {
            supports_interactive_components: true,
            supports_question_tool: true,
            supported_interaction_kinds: vec![QUESTION_INTERACTION_KIND.into()],
        }
    }

    pub fn non_interactive() -> Self {
        Self {
            supports_interactive_components: false,
            supports_question_tool: false,
            supported_interaction_kinds: Vec::new(),
        }
    }

    pub fn supports_interaction_kind(&self, kind: &str) -> bool {
        if !self.supports_interactive_components {
            return false;
        }

        self.supported_interaction_kinds.iter().any(|candidate| candidate == kind)
            || (kind == QUESTION_INTERACTION_KIND && self.supports_question_tool)
    }

    pub fn can_use_question_tool(&self) -> bool {
        self.supports_interaction_kind(QUESTION_INTERACTION_KIND)
    }

    pub fn can_use_interactive_tool(&self, interactive_kind: Option<&str>) -> bool {
        if !self.supports_interactive_components {
            return false;
        }

        match interactive_kind {
            Some(kind) => self.supports_interaction_kind(kind),
            None => true,
        }
    }
}
