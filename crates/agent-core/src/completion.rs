use serde::{Deserialize, Serialize};

use crate::{ConversationItem, ToolCall, ToolDefinition};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModelDisposition {
    Balanced,
    Precise,
    Fast,
    Creative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelLimit {
    pub context: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub provider: String,
    pub name: String,
    pub disposition: ModelDisposition,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub limit: Option<ModelLimit>,
}

impl ModelIdentity {
    pub fn new(
        provider: impl Into<String>,
        name: impl Into<String>,
        disposition: ModelDisposition,
    ) -> Self {
        Self {
            provider: provider.into(),
            name: name.into(),
            disposition,
            reasoning_effort: None,
            limit: None,
        }
    }

    pub fn with_reasoning_effort(mut self, effort: Option<String>) -> Self {
        self.reasoning_effort = effort;
        self
    }

    pub fn with_limit(mut self, limit: Option<ModelLimit>) -> Self {
        self.limit = limit;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelCheckpoint {
    pub protocol: String,
    pub token: String,
}

impl ModelCheckpoint {
    pub fn new(protocol: impl Into<String>, token: impl Into<String>) -> Self {
        Self { protocol: protocol.into(), token: token.into() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompletionSegment {
    Text(String),
    Thinking(String),
    ToolUse(ToolCall),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub segments: Vec<CompletionSegment>,
    #[serde(default)]
    pub checkpoint: Option<ModelCheckpoint>,
}

impl Completion {
    pub fn text(content: impl Into<String>) -> Self {
        Self { segments: vec![CompletionSegment::Text(content.into())], checkpoint: None }
    }

    pub fn plain_text(&self) -> String {
        self.segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::Text(text) => Some(text.as_str()),
                CompletionSegment::Thinking(_) | CompletionSegment::ToolUse(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn thinking_text(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::Thinking(text) => Some(text.as_str()),
                CompletionSegment::Text(_) | CompletionSegment::ToolUse(_) => None,
            })
            .collect();
        if parts.is_empty() { None } else { Some(parts.join("")) }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: ModelIdentity,
    pub instructions: Option<String>,
    pub conversation: Vec<ConversationItem>,
    pub resume_checkpoint: Option<ModelCheckpoint>,
    pub max_output_tokens: Option<u32>,
    pub available_tools: Vec<ToolDefinition>,
}
