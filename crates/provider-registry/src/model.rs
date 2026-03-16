use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProviderKind {
    OpenAiResponses,
    OpenAiChatCompletions,
}

impl ProviderKind {
    pub fn protocol_name(&self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChatCompletions => "openai-chat-completions",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelLimit {
    pub context: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub display_name: Option<String>,
    pub limit: Option<ModelLimit>,
    pub default_temperature: Option<f32>,
    #[serde(default)]
    pub supports_reasoning: bool,
    pub reasoning_effort: Option<String>,
}

impl ModelConfig {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            limit: None,
            default_temperature: None,
            supports_reasoning: false,
            reasoning_effort: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderProfile {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<ModelConfig>,
    pub active_model: Option<String>,
}

#[derive(Deserialize)]
struct ProviderProfileWire {
    name: String,
    kind: ProviderKind,
    base_url: String,
    api_key: String,
    #[serde(default)]
    models: Vec<ModelConfig>,
    #[serde(default)]
    active_model: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

impl<'de> Deserialize<'de> for ProviderProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ProviderProfileWire::deserialize(deserializer)?;
        let mut models = wire.models;
        if models.is_empty() && let Some(model) = wire.model {
            models.push(ModelConfig::new(model));
        }
        let active_model =
            wire.active_model.or_else(|| models.first().map(|model| model.id.clone()));

        Ok(Self {
            name: wire.name,
            kind: wire.kind,
            base_url: wire.base_url,
            api_key: wire.api_key,
            models,
            active_model,
        })
    }
}

impl ProviderProfile {
    pub fn openai_responses(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let model = model.into();
        Self {
            name: name.into(),
            kind: ProviderKind::OpenAiResponses,
            base_url: base_url.into(),
            api_key: api_key.into(),
            models: vec![ModelConfig::new(model.clone())],
            active_model: Some(model),
        }
    }

    pub fn openai_chat_completions(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let model = model.into();
        Self {
            name: name.into(),
            kind: ProviderKind::OpenAiChatCompletions,
            base_url: base_url.into(),
            api_key: api_key.into(),
            models: vec![ModelConfig::new(model.clone())],
            active_model: Some(model),
        }
    }

    pub fn has_model(&self, model_id: &str) -> bool {
        self.models.iter().any(|model| model.id == model_id)
    }

    pub fn active_model_id(&self) -> Option<&str> {
        self.active_model.as_deref().or_else(|| self.models.first().map(|model| model.id.as_str()))
    }

    pub fn active_model_config(&self) -> Option<&ModelConfig> {
        let active_id = self.active_model_id()?;
        self.models.iter().find(|model| model.id == active_id)
    }

    pub(crate) fn normalize_active_model(&mut self) {
        if self.active_model.as_deref().is_some_and(|model_id| self.has_model(model_id)) {
            return;
        }
        self.active_model = self.models.first().map(|model| model.id.clone());
    }
}
