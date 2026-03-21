#[cfg(test)]
#[path = "../../tests/model/mod.rs"]
mod tests;
mod trace;

use std::{sync::Arc, time::Instant};

use agent_core::{
    AbortSignal, Completion, CompletionRequest, CoreError, LanguageModel, ModelDisposition,
    ModelIdentity, ModelLimit, StreamEvent,
};
use agent_store::AiaStore;
use async_trait::async_trait;
use openai_adapter::{
    OpenAiAdapterError, OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel,
    OpenAiResponsesConfig, OpenAiResponsesModel,
};
use provider_registry::{ProviderKind, ProviderProfile};

use crate::reasoning::ReasoningEffort;
use trace::ModelTraceRecorder;

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderLaunchChoice {
    Bootstrap,
    OpenAi { profile: ProviderProfile, model: String, reasoning_effort: Option<String> },
}

pub struct ServerModel {
    inner: ServerModelInner,
    trace_store: Option<Arc<AiaStore>>,
}

enum ServerModelInner {
    Bootstrap(BootstrapModel),
    OpenAiResponses(OpenAiResponsesModel),
    OpenAiChatCompletions(OpenAiChatCompletionsModel),
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

#[derive(Debug)]
pub enum ServerModelError {
    Bootstrap(CoreError),
    OpenAi(OpenAiAdapterError),
}

impl ServerModelError {
    fn status_code(&self) -> Option<u16> {
        match self {
            Self::Bootstrap(_) => None,
            Self::OpenAi(error) => error.status_code(),
        }
    }

    fn response_body(&self) -> Option<&str> {
        match self {
            Self::Bootstrap(_) => None,
            Self::OpenAi(error) => error.response_body(),
        }
    }
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

struct BootstrapModel;

#[async_trait]
impl LanguageModel for BootstrapModel {
    type Error = CoreError;

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        _abort: &AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
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

        let completion = Completion::text(format!(
            "Bootstrap 模式收到：{latest_user}。请配置真实 provider 以使用完整功能。"
        ));
        sink(StreamEvent::Done);
        Ok(completion)
    }
}

#[async_trait]
impl LanguageModel for ServerModel {
    type Error = ServerModelError;

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        abort: &AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        CompletionTraceRunner::new(self, request, abort, sink).complete().await
    }

    fn is_cancelled_error(error: &Self::Error) -> bool {
        matches!(error, ServerModelError::OpenAi(openai) if openai.is_cancelled())
    }
}

impl ServerModel {
    fn new(inner: ServerModelInner, trace_store: Option<Arc<AiaStore>>) -> Self {
        Self { inner, trace_store }
    }
}

pub fn build_model_from_selection(
    selection: ProviderLaunchChoice,
    trace_store: Option<Arc<AiaStore>>,
) -> Result<(ModelIdentity, ServerModel), ServerSetupError> {
    match selection {
        ProviderLaunchChoice::Bootstrap => Ok((
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
            ServerModel::new(ServerModelInner::Bootstrap(BootstrapModel), trace_store),
        )),
        ProviderLaunchChoice::OpenAi { profile, model, reasoning_effort } => {
            build_openai_model(profile, &model, reasoning_effort, trace_store)
        }
    }
}

pub fn model_identity_from_selection(selection: &ProviderLaunchChoice) -> ModelIdentity {
    match selection {
        ProviderLaunchChoice::Bootstrap => {
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced)
        }
        ProviderLaunchChoice::OpenAi { profile, model, reasoning_effort } => {
            build_model_identity(profile, model, reasoning_effort.clone())
        }
    }
}

fn build_openai_model(
    profile: ProviderProfile,
    selected_model: &str,
    reasoning_effort: Option<String>,
    trace_store: Option<Arc<AiaStore>>,
) -> Result<(ModelIdentity, ServerModel), ServerSetupError> {
    let identity = build_model_identity(&profile, selected_model, reasoning_effort);
    let model_id = identity.name.clone();

    match profile.kind {
        ProviderKind::OpenAiResponses => {
            let config = OpenAiResponsesConfig::new(profile.base_url, profile.api_key, &model_id);
            let model =
                OpenAiResponsesModel::new(config).map_err(ServerSetupError::OpenAiAdapter)?;
            Ok((identity, ServerModel::new(ServerModelInner::OpenAiResponses(model), trace_store)))
        }
        ProviderKind::OpenAiChatCompletions => {
            let config =
                OpenAiChatCompletionsConfig::new(profile.base_url, profile.api_key, &model_id);
            let model =
                OpenAiChatCompletionsModel::new(config).map_err(ServerSetupError::OpenAiAdapter)?;
            Ok((
                identity,
                ServerModel::new(ServerModelInner::OpenAiChatCompletions(model), trace_store),
            ))
        }
    }
}

fn build_model_identity(
    profile: &ProviderProfile,
    selected_model: &str,
    reasoning_effort: Option<String>,
) -> ModelIdentity {
    let model_config = profile.models.iter().find(|model| model.id == selected_model);
    let model_id = model_config
        .map(|model| model.id.clone())
        .or_else(|| profile.default_model_id().map(str::to_string))
        .unwrap_or_default();
    let limit = model_config.and_then(|model| {
        model
            .limit
            .as_ref()
            .map(|limit| ModelLimit { context: limit.context, output: limit.output })
    });

    let normalized_reasoning_effort = ReasoningEffort::normalize_for_model(
        reasoning_effort,
        model_config.is_some_and(|model| model.supports_reasoning),
    );

    ModelIdentity::new("openai", &model_id, ModelDisposition::Balanced)
        .with_reasoning_effort(normalized_reasoning_effort)
        .with_limit(limit)
}

struct CompletionTraceRunner<'a> {
    model: &'a ServerModel,
    request: CompletionRequest,
    abort: &'a AbortSignal,
    sink: &'a mut (dyn FnMut(StreamEvent) + Send),
}

impl<'a> CompletionTraceRunner<'a> {
    fn new(
        model: &'a ServerModel,
        request: CompletionRequest,
        abort: &'a AbortSignal,
        sink: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Self {
        Self { model, request, abort, sink }
    }

    async fn complete(self) -> Result<Completion, ServerModelError> {
        let started_at_ms = trace::now_timestamp_ms();
        let mut trace_recorder =
            ModelTraceRecorder::new(self.model, &self.request, started_at_ms, true);
        let started = Instant::now();
        let mut traced_sink = |event: StreamEvent| {
            trace_recorder.observe(&event);
            (self.sink)(event);
        };

        let result = match &self.model.inner {
            ServerModelInner::Bootstrap(model) => model
                .complete_streaming(self.request, self.abort, &mut traced_sink)
                .await
                .map_err(ServerModelError::Bootstrap),
            ServerModelInner::OpenAiResponses(model) => model
                .complete_streaming(self.request, self.abort, &mut traced_sink)
                .await
                .map_err(ServerModelError::OpenAi),
            ServerModelInner::OpenAiChatCompletions(model) => model
                .complete_streaming(self.request, self.abort, &mut traced_sink)
                .await
                .map_err(ServerModelError::OpenAi),
        };

        trace_recorder.finish(started.elapsed(), &result);
        result
    }
}
