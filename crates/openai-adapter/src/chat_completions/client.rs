use agent_core::{AbortSignal, Completion, CompletionRequest, LanguageModel, StreamEvent};
use async_trait::async_trait;

use crate::{OpenAiAdapterError, stream_lines_with_abort};

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
        self.validate_request_model(&request)?;

        let client = self.http_client(&request)?;
        let response = self
            .apply_user_agent(
                client
                    .post(self.endpoint_url())
                    .bearer_auth(&self.config.api_key)
                    .json(&self.build_streaming_request_body(&request)),
                request.user_agent.as_deref(),
            )
            .send()
            .await
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
            return Err(self.request_failure(status, &body));
        }

        let mut state = ChatCompletionsStreamingState::default();
        stream_lines_with_abort(response, abort, sink, |line, sink| state.handle_line(line, sink))
            .await?;

        sink(StreamEvent::Done);
        Ok(state.into_completion(status.as_u16()))
    }

    fn is_cancelled_error(error: &Self::Error) -> bool {
        error.is_cancelled()
    }
}
