mod bootstrap;
#[cfg(test)]
mod tests;
mod trace;

use agent_core::{
    Completion, CompletionRequest, CoreError, LanguageModel, ModelDisposition, ModelIdentity,
    ModelLimit, StreamEvent,
};
use agent_store::{LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore};
use async_trait::async_trait;
use openai_adapter::{
    OpenAiAdapterError, OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel,
    OpenAiResponsesConfig, OpenAiResponsesModel,
};
use provider_registry::{ProviderKind, ProviderProfile};
use serde_json::{Value, json};
use std::{sync::Arc, time::Instant};

use bootstrap::BootstrapModel;
use trace::{
    TraceEventCollector, now_timestamp_ms, parse_status_code, request_summary, response_summary,
    stop_reason_label, trace_attributes, update_trace_attributes_from_completion,
    update_trace_attributes_from_error,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderLaunchChoice {
    Bootstrap,
    OpenAi(ProviderProfile),
}

pub struct ServerModel {
    inner: ServerModelInner,
    trace_store: Option<Arc<dyn LlmTraceStore>>,
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

    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        self.complete_with_trace(request, None, None).await
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        self.complete_with_trace(request, None, Some(sink)).await
    }

    async fn complete_streaming_with_abort(
        &self,
        request: CompletionRequest,
        abort: &agent_core::AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        self.complete_with_trace(request, Some(abort), Some(sink)).await
    }

    fn is_cancelled_error(error: &Self::Error) -> bool {
        matches!(error, ServerModelError::OpenAi(openai) if openai.is_cancelled())
    }
}

impl ServerModel {
    async fn complete_with_trace(
        &self,
        request: CompletionRequest,
        abort: Option<&agent_core::AbortSignal>,
        sink: Option<&mut (dyn FnMut(StreamEvent) + Send)>,
    ) -> Result<Completion, ServerModelError> {
        let started_at_ms = now_timestamp_ms();
        let trace_seed = self.trace_seed(&request, sink.is_some());
        let mut event_collector = TraceEventCollector::new(started_at_ms);
        let started = Instant::now();
        let result = match (&self.inner, abort, sink) {
            (ServerModelInner::Bootstrap(model), _, None) => {
                model.complete(request).await.map_err(ServerModelError::Bootstrap)
            }
            (ServerModelInner::Bootstrap(model), _, Some(sink)) => {
                let mut traced_sink = |event: StreamEvent| {
                    event_collector.observe(&event);
                    sink(event);
                };
                model
                    .complete_streaming(request, &mut traced_sink)
                    .await
                    .map_err(ServerModelError::Bootstrap)
            }
            (ServerModelInner::OpenAiResponses(model), None, None) => {
                model.complete(request).await.map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiResponses(model), None, Some(sink)) => {
                let mut traced_sink = |event: StreamEvent| {
                    event_collector.observe(&event);
                    sink(event);
                };
                model
                    .complete_streaming(request, &mut traced_sink)
                    .await
                    .map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiResponses(model), Some(abort), Some(sink)) => {
                let mut traced_sink = |event: StreamEvent| {
                    event_collector.observe(&event);
                    sink(event);
                };
                model
                    .complete_streaming_with_abort(request, abort, &mut traced_sink)
                    .await
                    .map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiResponses(model), Some(_), None) => {
                model.complete(request).await.map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiChatCompletions(model), None, None) => {
                model.complete(request).await.map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiChatCompletions(model), None, Some(sink)) => {
                let mut traced_sink = |event: StreamEvent| {
                    event_collector.observe(&event);
                    sink(event);
                };
                model
                    .complete_streaming(request, &mut traced_sink)
                    .await
                    .map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiChatCompletions(model), Some(abort), Some(sink)) => {
                let mut traced_sink = |event: StreamEvent| {
                    event_collector.observe(&event);
                    sink(event);
                };
                model
                    .complete_streaming_with_abort(request, abort, &mut traced_sink)
                    .await
                    .map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiChatCompletions(model), Some(_), None) => {
                model.complete(request).await.map_err(ServerModelError::OpenAi)
            }
        };

        self.persist_trace(trace_seed, started_at_ms, started.elapsed(), &result, event_collector);
        result
    }

    fn trace_seed(&self, request: &CompletionRequest, streaming: bool) -> Option<LlmTraceRecord> {
        let context = request.trace_context.as_ref()?;
        let (provider, protocol, base_url, endpoint_path, provider_request) = match &self.inner {
            ServerModelInner::Bootstrap(_) => {
                return None;
            }
            ServerModelInner::OpenAiResponses(model) => (
                "openai".to_string(),
                "openai-responses".to_string(),
                model.config().base_url.clone(),
                "/responses".to_string(),
                if streaming {
                    model.build_streaming_request_body(request)
                } else {
                    model.build_request_body(request)
                },
            ),
            ServerModelInner::OpenAiChatCompletions(model) => (
                "openai".to_string(),
                "openai-chat-completions".to_string(),
                model.config().base_url.clone(),
                "/chat/completions".to_string(),
                if streaming {
                    model.build_streaming_request_body(request)
                } else {
                    model.build_request_body(request)
                },
            ),
        };

        let otel_attributes = trace_attributes(
            request,
            context,
            provider.as_str(),
            base_url.as_str(),
            endpoint_path.as_str(),
            streaming,
        );

        Some(LlmTraceRecord {
            id: context.span_id.clone(),
            trace_id: context.trace_id.clone(),
            span_id: context.span_id.clone(),
            parent_span_id: context.parent_span_id.clone(),
            root_span_id: context.root_span_id.clone(),
            operation_name: context.operation_name.clone(),
            span_kind: LlmTraceSpanKind::Client,
            turn_id: context.turn_id.clone(),
            run_id: context.run_id.clone(),
            request_kind: context.request_kind.clone(),
            step_index: context.step_index,
            provider,
            protocol,
            model: request.model.name.clone(),
            base_url,
            endpoint_path,
            streaming,
            started_at_ms: 0,
            finished_at_ms: None,
            duration_ms: None,
            status_code: None,
            status: LlmTraceStatus::Succeeded,
            stop_reason: None,
            error: None,
            request_summary: request_summary(request),
            provider_request,
            response_summary: Value::Null,
            response_body: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cached_tokens: None,
            otel_attributes,
            events: vec![],
        })
    }

    fn persist_trace(
        &self,
        trace_seed: Option<LlmTraceRecord>,
        started_at_ms: u64,
        duration: std::time::Duration,
        result: &Result<Completion, ServerModelError>,
        mut event_collector: TraceEventCollector,
    ) {
        let Some(store) = self.trace_store.as_ref() else {
            return;
        };
        let Some(mut record) = trace_seed else {
            return;
        };

        record.started_at_ms = started_at_ms;
        record.finished_at_ms = Some(started_at_ms.saturating_add(duration.as_millis() as u64));
        record.duration_ms = Some(duration.as_millis() as u64);
        match result {
            Ok(completion) => {
                record.status = LlmTraceStatus::Succeeded;
                record.status_code = completion.http_status_code;
                record.stop_reason = Some(stop_reason_label(&completion.stop_reason));
                record.response_summary = response_summary(completion);
                record.response_body =
                    completion.response_body.clone().or_else(|| Some(completion.plain_text()));
                record.input_tokens = completion.usage.as_ref().map(|usage| usage.input_tokens);
                record.output_tokens = completion.usage.as_ref().map(|usage| usage.output_tokens);
                record.total_tokens = completion.usage.as_ref().map(|usage| usage.total_tokens);
                record.cached_tokens = completion.usage.as_ref().map(|usage| usage.cached_tokens);
                update_trace_attributes_from_completion(&mut record, completion);
                event_collector.finish_success(&mut record, completion);
            }
            Err(error) => {
                record.status = LlmTraceStatus::Failed;
                record.error = Some(error.to_string());
                record.status_code =
                    error.status_code().or_else(|| parse_status_code(error.to_string().as_str()));
                record.response_body = error.response_body().map(std::string::ToString::to_string);
                record.response_summary = json!({
                    "error": error.to_string(),
                    "http_status_code": record.status_code,
                });
                update_trace_attributes_from_error(&mut record);
                event_collector.finish_error(&mut record);
            }
        }

        if let Err(error) = store.record(&record) {
            eprintln!("trace record failed: {error}");
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
    trace_store: Option<Arc<dyn LlmTraceStore>>,
) -> Result<(ModelIdentity, ServerModel), ServerSetupError> {
    match selection {
        ProviderLaunchChoice::Bootstrap => Ok((
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
            ServerModel { inner: ServerModelInner::Bootstrap(BootstrapModel), trace_store },
        )),
        ProviderLaunchChoice::OpenAi(profile) => {
            let model_config = profile.active_model_config();
            let model_id = model_config
                .map(|m| m.id.clone())
                .or_else(|| profile.active_model.clone())
                .unwrap_or_default();
            let reasoning_effort = model_config.and_then(|m| m.reasoning_effort.clone());
            let limit = model_config.and_then(|m| {
                m.limit
                    .as_ref()
                    .map(|limit| ModelLimit { context: limit.context, output: limit.output })
            });
            let identity = ModelIdentity::new("openai", &model_id, ModelDisposition::Balanced)
                .with_reasoning_effort(reasoning_effort)
                .with_limit(limit);
            match profile.kind {
                ProviderKind::OpenAiResponses => {
                    let config =
                        OpenAiResponsesConfig::new(profile.base_url, profile.api_key, &model_id);
                    let model = OpenAiResponsesModel::new(config)
                        .map_err(ServerSetupError::OpenAiAdapter)?;
                    Ok((
                        identity,
                        ServerModel {
                            inner: ServerModelInner::OpenAiResponses(model),
                            trace_store,
                        },
                    ))
                }
                ProviderKind::OpenAiChatCompletions => {
                    let config = OpenAiChatCompletionsConfig::new(
                        profile.base_url,
                        profile.api_key,
                        &model_id,
                    );
                    let model = OpenAiChatCompletionsModel::new(config)
                        .map_err(ServerSetupError::OpenAiAdapter)?;
                    Ok((
                        identity,
                        ServerModel {
                            inner: ServerModelInner::OpenAiChatCompletions(model),
                            trace_store,
                        },
                    ))
                }
            }
        }
    }
}
