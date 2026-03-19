mod bootstrap;
mod factory;
mod runner;
#[cfg(test)]
mod tests;
mod trace;

use agent_core::{
    AbortSignal, Completion, CompletionRequest, CoreError, LanguageModel, ModelIdentity,
    StreamEvent,
};
use agent_store::AiaStore;
use async_trait::async_trait;
use openai_adapter::{OpenAiAdapterError, OpenAiChatCompletionsModel, OpenAiResponsesModel};
use provider_registry::ProviderProfile;
use std::sync::Arc;

use bootstrap::BootstrapModel;
use factory::ModelFactory;
pub use factory::ServerSetupError;
pub use factory::model_identity_from_selection;
use runner::CompletionTraceRunner;

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderLaunchChoice {
    Bootstrap,
    OpenAi(ProviderProfile),
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
    ModelFactory::new(trace_store).build(selection)
}
