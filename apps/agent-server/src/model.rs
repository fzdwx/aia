use agent_core::{
    Completion, CompletionRequest, CoreError, LanguageModel, ModelDisposition, ModelIdentity,
    StreamEvent,
};
use openai_adapter::{
    OpenAiAdapterError, OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel,
    OpenAiResponsesConfig, OpenAiResponsesModel,
};
use provider_registry::{ProviderKind, ProviderProfile};

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderLaunchChoice {
    Bootstrap,
    OpenAi(ProviderProfile),
}

pub enum ServerModel {
    Bootstrap(BootstrapModel),
    OpenAiResponses(OpenAiResponsesModel),
    OpenAiChatCompletions(OpenAiChatCompletionsModel),
}

#[derive(Debug)]
pub enum ServerModelError {
    Bootstrap(CoreError),
    OpenAi(OpenAiAdapterError),
}

impl std::fmt::Display for ServerModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bootstrap(error) => write!(f, "{error}"),
            Self::OpenAi(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ServerModelError {}

impl LanguageModel for ServerModel {
    type Error = ServerModelError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        match self {
            Self::Bootstrap(model) => model.complete(request).map_err(ServerModelError::Bootstrap),
            Self::OpenAiResponses(model) => {
                model.complete(request).map_err(ServerModelError::OpenAi)
            }
            Self::OpenAiChatCompletions(model) => {
                model.complete(request).map_err(ServerModelError::OpenAi)
            }
        }
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        match self {
            Self::Bootstrap(model) => {
                model.complete_streaming(request, sink).map_err(ServerModelError::Bootstrap)
            }
            Self::OpenAiResponses(model) => {
                model.complete_streaming(request, sink).map_err(ServerModelError::OpenAi)
            }
            Self::OpenAiChatCompletions(model) => {
                model.complete_streaming(request, sink).map_err(ServerModelError::OpenAi)
            }
        }
    }
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

pub fn build_model_from_selection(
    selection: ProviderLaunchChoice,
) -> Result<(ModelIdentity, ServerModel), ServerSetupError> {
    match selection {
        ProviderLaunchChoice::Bootstrap => Ok((
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
            ServerModel::Bootstrap(BootstrapModel),
        )),
        ProviderLaunchChoice::OpenAi(profile) => {
            let model_config = profile.active_model_config();
            let model_id = model_config
                .map(|m| m.id.clone())
                .or_else(|| profile.active_model.clone())
                .unwrap_or_default();
            let reasoning_effort = model_config.and_then(|m| m.reasoning_effort.clone());
            let identity = ModelIdentity::new("openai", &model_id, ModelDisposition::Balanced)
                .with_reasoning_effort(reasoning_effort);
            match profile.kind {
                ProviderKind::OpenAiResponses => {
                    let config =
                        OpenAiResponsesConfig::new(profile.base_url, profile.api_key, &model_id);
                    let model = OpenAiResponsesModel::new(config)
                        .map_err(ServerSetupError::OpenAiAdapter)?;
                    Ok((identity, ServerModel::OpenAiResponses(model)))
                }
                ProviderKind::OpenAiChatCompletions => {
                    let config = OpenAiChatCompletionsConfig::new(
                        profile.base_url,
                        profile.api_key,
                        &model_id,
                    );
                    let model = OpenAiChatCompletionsModel::new(config)
                        .map_err(ServerSetupError::OpenAiAdapter)?;
                    Ok((identity, ServerModel::OpenAiChatCompletions(model)))
                }
            }
        }
    }
}

pub struct BootstrapModel;

impl LanguageModel for BootstrapModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let latest_user = request
            .conversation
            .iter()
            .rev()
            .find_map(|item| {
                item.as_message()
                    .filter(|message| message.role == agent_core::Role::User)
                    .map(|message| message.content.clone())
            })
            .unwrap_or_else(|| "空输入".into());

        Ok(Completion::text(format!(
            "Bootstrap 模式收到：{latest_user}。请配置真实 provider 以使用完整功能。"
        )))
    }
}
