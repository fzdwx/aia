use agent_core::{AbortSignal, Completion, CompletionRequest, LanguageModel, StreamEvent};
use async_trait::async_trait;

use crate::{
    OpenAiAdapterError, http::validate_request_model, streaming::complete_streaming_request,
};

use super::{OpenAiChatCompletionsModel, streaming::ChatCompletionsStreamingState};

#[async_trait]
impl LanguageModel for OpenAiChatCompletionsModel {
    type Error = OpenAiAdapterError;

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        abort: &AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        validate_request_model(&self.config().model, &request)?;

        let endpoint_url = self.endpoint_url();
        let request_body = self.build_streaming_request_body(&request);
        complete_streaming_request::<ChatCompletionsStreamingState>(
            &endpoint_url,
            &self.config.api_key,
            &request,
            request_body,
            abort,
            sink,
        )
        .await
    }

    fn is_cancelled_error(error: &Self::Error) -> bool {
        error.is_cancelled()
    }
}
