mod client;
#[cfg(test)]
mod parsing;
mod payloads;
mod request;
mod streaming;

use crate::OpenAiAdapterError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiResponsesConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiResponsesConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self { base_url: base_url.into(), api_key: api_key.into(), model: model.into() }
    }
}

pub struct OpenAiResponsesModel {
    config: OpenAiResponsesConfig,
}

impl OpenAiResponsesModel {
    pub fn new(config: OpenAiResponsesConfig) -> Result<Self, OpenAiAdapterError> {
        if config.base_url.is_empty() || config.api_key.is_empty() || config.model.is_empty() {
            return Err(OpenAiAdapterError::new("配置缺失"));
        }
        Ok(Self { config })
    }

    pub fn config(&self) -> &OpenAiResponsesConfig {
        &self.config
    }
}
