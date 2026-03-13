use std::{fmt, fs, path::Path};

use serde::{Deserialize, Serialize};

pub fn default_registry_path() -> std::path::PathBuf {
    std::path::PathBuf::from(".aia/providers.json")
}

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub display_name: Option<String>,
    pub context_window: Option<u32>,
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
            context_window: None,
            default_temperature: None,
            supports_reasoning: false,
            reasoning_effort: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<ModelConfig>,
    pub active_model: Option<String>,
}

impl ProviderProfile {
    pub fn openai_responses(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let model_id = model.into();
        Self {
            name: name.into(),
            kind: ProviderKind::OpenAiResponses,
            base_url: base_url.into(),
            api_key: api_key.into(),
            models: vec![ModelConfig::new(&model_id)],
            active_model: Some(model_id),
        }
    }

    pub fn openai_chat_completions(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let model_id = model.into();
        Self {
            name: name.into(),
            kind: ProviderKind::OpenAiChatCompletions,
            base_url: base_url.into(),
            api_key: api_key.into(),
            models: vec![ModelConfig::new(&model_id)],
            active_model: Some(model_id),
        }
    }

    pub fn active_model_config(&self) -> Option<&ModelConfig> {
        let active_id = self.active_model.as_ref()?;
        self.models.iter().find(|m| m.id == *active_id)
    }

    pub fn active_model_id(&self) -> Option<&str> {
        self.active_model.as_deref()
    }

    pub fn has_model(&self, model_id: &str) -> bool {
        self.models.iter().any(|m| m.id == model_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: Vec<ProviderProfile>,
    active_provider: Option<String>,
}

impl ProviderRegistry {
    pub fn load_or_default(path: &Path) -> Result<Self, ProviderRegistryError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
        serde_json::from_str(&contents)
            .map_err(|error| ProviderRegistryError::new(error.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), ProviderRegistryError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
        }

        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
        fs::write(path, contents).map_err(|error| ProviderRegistryError::new(error.to_string()))
    }

    pub fn upsert(&mut self, provider: ProviderProfile) {
        if let Some(existing) =
            self.providers.iter_mut().find(|existing| existing.name == provider.name)
        {
            *existing = provider;
            return;
        }

        if self.active_provider.is_none() {
            self.active_provider = Some(provider.name.clone());
        }

        self.providers.push(provider);
    }

    pub fn remove(&mut self, name: &str) -> Result<(), ProviderRegistryError> {
        let before = self.providers.len();
        self.providers.retain(|p| p.name != name);
        if self.providers.len() == before {
            return Err(ProviderRegistryError::new(format!("provider 不存在：{name}")));
        }
        if self.active_provider.as_deref() == Some(name) {
            self.active_provider = self.providers.first().map(|p| p.name.clone());
        }
        Ok(())
    }

    pub fn set_active(&mut self, provider_name: &str) -> Result<(), ProviderRegistryError> {
        if !self.providers.iter().any(|provider| provider.name == provider_name) {
            return Err(ProviderRegistryError::new(format!("provider 不存在：{provider_name}")));
        }

        self.active_provider = Some(provider_name.to_string());
        Ok(())
    }

    pub fn active_provider(&self) -> Option<&ProviderProfile> {
        let active_name = self.active_provider.as_ref()?;
        self.providers.iter().find(|provider| provider.name == *active_name)
    }

    pub fn providers(&self) -> &[ProviderProfile] {
        &self.providers
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProviderRegistryError {
    message: String,
}

impl ProviderRegistryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for ProviderRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProviderRegistryError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{ModelConfig, ProviderProfile, ProviderRegistry};

    fn temp_file(name: &str) -> PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
        std::env::temp_dir().join(format!("aia-{name}-{suffix}.json"))
    }

    #[test]
    fn 默认存储路径位于项目隐藏目录() {
        assert_eq!(super::default_registry_path(), PathBuf::from(".aia/providers.json"));
    }

    #[test]
    fn 可保存并重新载入注册表() {
        let path = temp_file("provider-registry");
        let mut registry = ProviderRegistry::default();
        registry.upsert(ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        registry.set_active("main").expect("设置活动 provider 成功");

        registry.save(&path).expect("保存成功");
        let restored = ProviderRegistry::load_or_default(&path).expect("加载成功");

        assert_eq!(restored.providers().len(), 1);
        assert_eq!(restored.active_provider().map(|provider| provider.name.as_str()), Some("main"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn 同名_provider_会被更新而不是重复追加() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        registry.upsert(ProviderProfile::openai_responses(
            "main",
            "https://example.com/v1",
            "secret-2",
            "gpt-4.1",
        ));

        assert_eq!(registry.providers().len(), 1);
        assert_eq!(registry.providers()[0].base_url, "https://example.com/v1");
        assert_eq!(registry.providers()[0].active_model_id(), Some("gpt-4.1"));
    }

    #[test]
    fn 设置不存在的活动_provider_会报错() {
        let mut registry = ProviderRegistry::default();

        let error = registry.set_active("missing").expect_err("应当失败");

        assert!(error.to_string().contains("不存在"));
    }

    #[test]
    fn 可构造_openai_兼容聊天补全_provider() {
        let provider = ProviderProfile::openai_chat_completions(
            "compat",
            "http://127.0.0.1:8000/v1",
            "secret",
            "minum-security-llm",
        );

        assert_eq!(provider.kind, super::ProviderKind::OpenAiChatCompletions);
        assert_eq!(provider.name, "compat");
        assert_eq!(provider.base_url, "http://127.0.0.1:8000/v1");
        assert_eq!(provider.active_model_id(), Some("minum-security-llm"));
    }

    #[test]
    fn 多模型_provider_可查找活动模型配置() {
        let profile = ProviderProfile {
            name: "test".into(),
            kind: super::ProviderKind::OpenAiResponses,
            base_url: "https://api.openai.com/v1".into(),
            api_key: "secret".into(),
            models: vec![
                ModelConfig::new("gpt-4.1-mini"),
                ModelConfig {
                    id: "o3-mini".into(),
                    display_name: Some("O3 Mini".into()),
                    context_window: Some(200_000),
                    default_temperature: None,
                    supports_reasoning: true,
                    reasoning_effort: Some("medium".into()),
                },
            ],
            active_model: Some("o3-mini".into()),
        };

        let config = profile.active_model_config().expect("应有活动模型");
        assert_eq!(config.id, "o3-mini");
        assert!(config.supports_reasoning);
        assert_eq!(config.reasoning_effort.as_deref(), Some("medium"));
    }

    #[test]
    fn 删除_provider_成功() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        registry.upsert(ProviderProfile::openai_responses(
            "secondary",
            "https://api.openai.com/v1",
            "secret-2",
            "gpt-4.1",
        ));
        registry.set_active("main").unwrap();

        registry.remove("main").expect("删除成功");

        assert_eq!(registry.providers().len(), 1);
        assert_eq!(registry.active_provider().map(|p| p.name.as_str()), Some("secondary"));
    }

    #[test]
    fn 删除不存在的_provider_报错() {
        let mut registry = ProviderRegistry::default();
        let err = registry.remove("nope").expect_err("应当失败");
        assert!(err.to_string().contains("不存在"));
    }

    #[test]
    fn has_model_检查模型是否存在() {
        let profile = ProviderProfile::openai_responses(
            "test",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        );
        assert!(profile.has_model("gpt-4.1-mini"));
        assert!(!profile.has_model("gpt-4.1"));
    }
}
