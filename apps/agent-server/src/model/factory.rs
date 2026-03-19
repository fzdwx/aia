use std::sync::Arc;

use agent_core::{ModelDisposition, ModelIdentity, ModelLimit};
use agent_store::AiaStore;
use openai_adapter::{
    OpenAiAdapterError, OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel,
    OpenAiResponsesConfig, OpenAiResponsesModel,
};
use provider_registry::{ProviderKind, ProviderProfile};

use super::{ProviderLaunchChoice, ServerModel, ServerModelInner};

pub(super) struct ModelFactory {
    trace_store: Option<Arc<AiaStore>>,
}

impl ModelFactory {
    pub(super) fn new(trace_store: Option<Arc<AiaStore>>) -> Self {
        Self { trace_store }
    }

    pub(super) fn build(
        self,
        selection: ProviderLaunchChoice,
    ) -> Result<(ModelIdentity, ServerModel), ServerSetupError> {
        match selection {
            ProviderLaunchChoice::Bootstrap => Ok((
                ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
                ServerModel::new(
                    ServerModelInner::Bootstrap(super::BootstrapModel),
                    self.trace_store,
                ),
            )),
            ProviderLaunchChoice::OpenAi(profile) => self.build_openai(profile),
        }
    }

    fn build_openai(
        self,
        profile: ProviderProfile,
    ) -> Result<(ModelIdentity, ServerModel), ServerSetupError> {
        let identity = build_model_identity(&profile);
        let model_id = identity.name.clone();

        match profile.kind {
            ProviderKind::OpenAiResponses => {
                let config =
                    OpenAiResponsesConfig::new(profile.base_url, profile.api_key, &model_id);
                let model =
                    OpenAiResponsesModel::new(config).map_err(ServerSetupError::OpenAiAdapter)?;
                Ok((
                    identity,
                    ServerModel::new(ServerModelInner::OpenAiResponses(model), self.trace_store),
                ))
            }
            ProviderKind::OpenAiChatCompletions => {
                let config =
                    OpenAiChatCompletionsConfig::new(profile.base_url, profile.api_key, &model_id);
                let model = OpenAiChatCompletionsModel::new(config)
                    .map_err(ServerSetupError::OpenAiAdapter)?;
                Ok((
                    identity,
                    ServerModel::new(
                        ServerModelInner::OpenAiChatCompletions(model),
                        self.trace_store,
                    ),
                ))
            }
        }
    }
}

pub fn model_identity_from_selection(selection: &ProviderLaunchChoice) -> ModelIdentity {
    match selection {
        ProviderLaunchChoice::Bootstrap => {
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced)
        }
        ProviderLaunchChoice::OpenAi(profile) => build_model_identity(profile),
    }
}

fn build_model_identity(profile: &ProviderProfile) -> ModelIdentity {
    let model_config = profile.active_model_config();
    let model_id = model_config
        .map(|model| model.id.clone())
        .or_else(|| profile.active_model.clone())
        .unwrap_or_default();
    let reasoning_effort = model_config.and_then(|model| model.reasoning_effort.clone());
    let limit = model_config.and_then(|model| {
        model
            .limit
            .as_ref()
            .map(|limit| ModelLimit { context: limit.context, output: limit.output })
    });

    ModelIdentity::new("openai", &model_id, ModelDisposition::Balanced)
        .with_reasoning_effort(reasoning_effort)
        .with_limit(limit)
}

#[derive(Debug)]
pub enum ServerSetupError {
    OpenAiAdapter(OpenAiAdapterError),
}

impl std::fmt::Display for ServerSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenAiAdapter(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ServerSetupError {}
