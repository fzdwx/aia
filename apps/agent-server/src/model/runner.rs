use std::time::Instant;

use agent_core::{AbortSignal, Completion, CompletionRequest, LanguageModel, StreamEvent};

use super::{ServerModel, ServerModelError, ServerModelInner, trace::ModelTraceRecorder};

pub(super) struct CompletionTraceRunner<'a> {
    model: &'a ServerModel,
    request: CompletionRequest,
    abort: &'a AbortSignal,
    sink: &'a mut (dyn FnMut(StreamEvent) + Send),
}

impl<'a> CompletionTraceRunner<'a> {
    pub(super) fn new(
        model: &'a ServerModel,
        request: CompletionRequest,
        abort: &'a AbortSignal,
        sink: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Self {
        Self { model, request, abort, sink }
    }

    pub(super) async fn complete(self) -> Result<Completion, ServerModelError> {
        let started_at_ms = super::trace::now_timestamp_ms();
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
