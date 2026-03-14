use agent_core::{
    Completion, CompletionRequest, CompletionStopReason, CoreError, LanguageModel,
    ModelDisposition, ModelIdentity, ModelLimit, StreamEvent,
};
use llm_trace::{LlmTraceRecord, LlmTraceStatus, LlmTraceStore};
use openai_adapter::{
    OpenAiAdapterError, OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel,
    OpenAiResponsesConfig, OpenAiResponsesModel,
};
use provider_registry::{ProviderKind, ProviderProfile};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
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

impl LanguageModel for ServerModel {
    type Error = ServerModelError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        self.complete_with_trace(request, None)
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        self.complete_with_trace(request, Some(sink))
    }
}

impl ServerModel {
    #[cfg(test)]
    pub fn bootstrap() -> Self {
        Self { inner: ServerModelInner::Bootstrap(BootstrapModel), trace_store: None }
    }

    fn complete_with_trace(
        &self,
        request: CompletionRequest,
        sink: Option<&mut dyn FnMut(StreamEvent)>,
    ) -> Result<Completion, ServerModelError> {
        let trace_seed = self.trace_seed(&request, sink.is_some());
        let started_at_ms = now_timestamp_ms();
        let started = Instant::now();
        let result = match (&self.inner, sink) {
            (ServerModelInner::Bootstrap(model), None) => {
                model.complete(request).map_err(ServerModelError::Bootstrap)
            }
            (ServerModelInner::Bootstrap(model), Some(sink)) => {
                model.complete_streaming(request, sink).map_err(ServerModelError::Bootstrap)
            }
            (ServerModelInner::OpenAiResponses(model), None) => {
                model.complete(request).map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiResponses(model), Some(sink)) => {
                model.complete_streaming(request, sink).map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiChatCompletions(model), None) => {
                model.complete(request).map_err(ServerModelError::OpenAi)
            }
            (ServerModelInner::OpenAiChatCompletions(model), Some(sink)) => {
                model.complete_streaming(request, sink).map_err(ServerModelError::OpenAi)
            }
        };

        self.persist_trace(trace_seed, started_at_ms, started.elapsed(), &result);
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

        Some(LlmTraceRecord {
            id: context.trace_id.clone(),
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
        })
    }

    fn persist_trace(
        &self,
        trace_seed: Option<LlmTraceRecord>,
        started_at_ms: u64,
        duration: std::time::Duration,
        result: &Result<Completion, ServerModelError>,
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

fn request_summary(request: &CompletionRequest) -> Value {
    json!({
        "has_instructions": request.instructions.as_ref().is_some_and(|value| !value.is_empty()),
        "conversation_items": request.conversation.len(),
        "tool_names": request.available_tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>(),
        "max_output_tokens": request.max_output_tokens,
    })
}

fn response_summary(completion: &Completion) -> Value {
    json!({
        "assistant_text": completion.plain_text(),
        "thinking_text": completion.thinking_text(),
        "stop_reason": stop_reason_label(&completion.stop_reason),
        "usage": completion.usage.as_ref().map(|usage| json!({
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens,
        })),
    })
}

fn stop_reason_label(reason: &CompletionStopReason) -> String {
    match reason {
        CompletionStopReason::Stop => "stop".to_string(),
        CompletionStopReason::ToolUse => "tool_use".to_string(),
        CompletionStopReason::MaxTokens => "max_tokens".to_string(),
        CompletionStopReason::ContentFilter => "content_filter".to_string(),
        CompletionStopReason::Unknown(value) => value.clone(),
    }
}

fn parse_status_code(error: &str) -> Option<u16> {
    error
        .split(" -> ")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|value| value.parse::<u16>().ok())
}

fn now_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        thread,
    };

    use agent_core::{
        CompletionRequest, ConversationItem, LanguageModel, Message, ModelDisposition,
        ModelIdentity, Role,
    };
    use llm_trace::{LlmTraceStore, SqliteLlmTraceStore};
    use provider_registry::{ModelConfig, ModelLimit, ProviderKind, ProviderProfile};

    use super::{ProviderLaunchChoice, build_model_from_selection};

    #[test]
    fn responses_model_call_writes_llm_trace_record() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept should succeed");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("request should be readable");

            let body = [
                r#"data: {"type":"response.created","response":{"id":"resp_1"}}"#,
                r#"data: {"type":"response.output_text.delta","delta":"trace ok"}"#,
                r#"data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","usage":{"input_tokens":21,"output_tokens":9,"total_tokens":30}}}"#,
                r#"data: [DONE]"#,
            ]
            .join("\n\n");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}\n\n",
                body
            );
            stream.write_all(response.as_bytes()).expect("response write should succeed");
        });

        let store = Arc::new(SqliteLlmTraceStore::in_memory().expect("trace store should init"));
        let profile = ProviderProfile {
            name: "rayin".to_string(),
            kind: ProviderKind::OpenAiResponses,
            base_url: format!("http://{address}"),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: "gpt-5.4".to_string(),
                display_name: None,
                limit: Some(ModelLimit { context: Some(200_000), output: Some(8_192) }),
                default_temperature: None,
                supports_reasoning: false,
                reasoning_effort: None,
            }],
            active_model: Some("gpt-5.4".to_string()),
        };

        let (identity, model) =
            build_model_from_selection(ProviderLaunchChoice::OpenAi(profile), Some(store.clone()))
                .expect("model should build");

        let completion = model
            .complete_streaming(
                CompletionRequest {
                    model: ModelIdentity::new("openai", "gpt-5.4", ModelDisposition::Balanced),
                    instructions: Some("保持简洁".into()),
                    conversation: vec![ConversationItem::Message(Message::new(Role::User, "hi"))],
                    max_output_tokens: Some(128),
                    available_tools: vec![],
                    trace_context: Some(agent_core::LlmTraceRequestContext {
                        trace_id: "trace-1".into(),
                        turn_id: "turn-1".into(),
                        run_id: "turn-1".into(),
                        request_kind: "completion".into(),
                        step_index: 0,
                    }),
                },
                &mut |_| {},
            )
            .expect("completion should succeed");

        handle.join().expect("server thread should exit");
        assert_eq!(identity.name, "gpt-5.4");
        assert_eq!(completion.plain_text(), "trace ok");

        let trace =
            store.get("trace-1").expect("trace query should succeed").expect("trace exists");
        assert_eq!(trace.model, "gpt-5.4");
        assert_eq!(trace.endpoint_path, "/responses");
        assert_eq!(trace.status_code, Some(200));
        assert_eq!(trace.input_tokens, Some(21));
        assert_eq!(trace.output_tokens, Some(9));
        assert_eq!(trace.total_tokens, Some(30));
        assert!(
            trace.response_body.as_deref().is_some_and(|body| body.contains("response.completed"))
        );
    }

    #[test]
    fn responses_http_502_writes_failed_trace_record() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept should succeed");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("request should be readable");

            let body = r#"{"error":"gateway failure"}"#;
            let response = format!(
                "HTTP/1.1 502 Bad Gateway\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("response write should succeed");
        });

        let store = Arc::new(SqliteLlmTraceStore::in_memory().expect("trace store should init"));
        let profile = ProviderProfile {
            name: "rayin".to_string(),
            kind: ProviderKind::OpenAiResponses,
            base_url: format!("http://{address}"),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: "gpt-5.4".to_string(),
                display_name: None,
                limit: Some(ModelLimit { context: Some(200_000), output: Some(8_192) }),
                default_temperature: None,
                supports_reasoning: false,
                reasoning_effort: None,
            }],
            active_model: Some("gpt-5.4".to_string()),
        };

        let (_identity, model) =
            build_model_from_selection(ProviderLaunchChoice::OpenAi(profile), Some(store.clone()))
                .expect("model should build");

        let error = model
            .complete(CompletionRequest {
                model: ModelIdentity::new("openai", "gpt-5.4", ModelDisposition::Balanced),
                instructions: Some("保持简洁".into()),
                conversation: vec![ConversationItem::Message(Message::new(Role::User, "hi"))],
                max_output_tokens: Some(128),
                available_tools: vec![],
                trace_context: Some(agent_core::LlmTraceRequestContext {
                    trace_id: "trace-502".into(),
                    turn_id: "turn-1".into(),
                    run_id: "turn-1".into(),
                    request_kind: "completion".into(),
                    step_index: 0,
                }),
            })
            .expect_err("completion should fail");

        handle.join().expect("server thread should exit");
        assert!(error.to_string().contains("502"));

        let trace =
            store.get("trace-502").expect("trace query should succeed").expect("trace exists");
        assert_eq!(trace.status, llm_trace::LlmTraceStatus::Failed);
        assert_eq!(trace.status_code, Some(502));
        assert!(
            trace.response_body.as_deref().is_some_and(|body| body.contains("gateway failure"))
        );
    }
}
