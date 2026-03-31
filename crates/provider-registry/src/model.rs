use agent_core::{ModelLimit, ModelRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AdapterKind {
    OpenAiResponses,
    OpenAiChatCompletions,
}

impl AdapterKind {
    pub fn protocol_name(&self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChatCompletions => "openai-chat-completions",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub display_name: Option<String>,
    pub limit: Option<ModelLimit>,
    pub default_temperature: Option<f32>,
    #[serde(default)]
    pub supports_reasoning: bool,
}

impl ModelConfig {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            limit: None,
            default_temperature: None,
            supports_reasoning: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialRef {
    ApiKey { value: String },
}

impl CredentialRef {
    pub fn api_key(value: impl Into<String>) -> Self {
        Self::ApiKey { value: value.into() }
    }

    pub fn api_key_value(&self) -> &str {
        match self {
            Self::ApiKey { value } => value,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key_value().trim().is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderEndpoint {
    pub base_url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderAccount {
    pub id: String,
    pub label: String,
    pub adapter: AdapterKind,
    pub endpoint: ProviderEndpoint,
    pub credential: CredentialRef,
    pub models: Vec<ModelConfig>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedModelSpec {
    pub model_ref: ModelRef,
    pub adapter: AdapterKind,
    pub base_url: String,
    pub credential: CredentialRef,
    pub model: ModelConfig,
}

impl ProviderAccount {
    pub fn openai_responses(
        id: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let model = model.into();
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            adapter: AdapterKind::OpenAiResponses,
            endpoint: ProviderEndpoint { base_url: base_url.into() },
            credential: CredentialRef::api_key(api_key),
            models: vec![ModelConfig::new(model.clone())],
        }
    }

    pub fn openai_chat_completions(
        id: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let model = model.into();
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            adapter: AdapterKind::OpenAiChatCompletions,
            endpoint: ProviderEndpoint { base_url: base_url.into() },
            credential: CredentialRef::api_key(api_key),
            models: vec![ModelConfig::new(model.clone())],
        }
    }

    pub fn has_model(&self, model_id: &str) -> bool {
        self.models.iter().any(|model| model.id == model_id)
    }

    pub fn default_model_id(&self) -> Option<&str> {
        self.models.first().map(|model| model.id.as_str())
    }

    pub fn default_model_config(&self) -> Option<&ModelConfig> {
        self.models.first()
    }

    pub fn model_ref(&self, model_id: impl Into<String>) -> ModelRef {
        ModelRef::new(self.id.clone(), model_id)
    }
}
