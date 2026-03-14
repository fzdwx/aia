use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{KeepAlive, Sse},
    },
};
use llm_trace::LlmTraceStoreError;
use serde::{Deserialize, Serialize};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use provider_registry::{ModelConfig, ModelLimit, ProviderKind};

use crate::{
    runtime_worker::{
        CreateProviderInput, ProviderInfoSnapshot, RuntimeWorkerError, SwitchProviderInput,
        UpdateProviderInput,
    },
    sse::{SsePayload, TurnStatus},
    state::SharedState,
};

#[derive(Deserialize)]
pub struct TurnRequest {
    pub prompt: String,
}

#[derive(Deserialize)]
pub struct HandoffRequest {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

#[derive(Serialize)]
pub struct ProviderListItem {
    pub name: String,
    pub kind: String,
    pub models: Vec<ModelConfigDto>,
    pub active_model: Option<String>,
    pub base_url: String,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModelLimitDto {
    pub context: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModelConfigDto {
    pub id: String,
    pub display_name: Option<String>,
    pub limit: Option<ModelLimitDto>,
    pub default_temperature: Option<f32>,
    #[serde(default)]
    pub supports_reasoning: bool,
    pub reasoning_effort: Option<String>,
}

impl From<&ModelLimit> for ModelLimitDto {
    fn from(limit: &ModelLimit) -> Self {
        Self { context: limit.context, output: limit.output }
    }
}

impl From<ModelLimitDto> for ModelLimit {
    fn from(dto: ModelLimitDto) -> Self {
        Self { context: dto.context, output: dto.output }
    }
}

impl From<&ModelConfig> for ModelConfigDto {
    fn from(m: &ModelConfig) -> Self {
        Self {
            id: m.id.clone(),
            display_name: m.display_name.clone(),
            limit: m.limit.as_ref().map(ModelLimitDto::from),
            default_temperature: m.default_temperature,
            supports_reasoning: m.supports_reasoning,
            reasoning_effort: m.reasoning_effort.clone(),
        }
    }
}

impl From<ModelConfigDto> for ModelConfig {
    fn from(dto: ModelConfigDto) -> Self {
        Self {
            id: dto.id,
            display_name: dto.display_name,
            limit: dto.limit.map(ModelLimit::from),
            default_temperature: dto.default_temperature,
            supports_reasoning: dto.supports_reasoning,
            reasoning_effort: dto.reasoning_effort,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub kind: String,
    pub models: Vec<ModelConfigDto>,
    pub active_model: Option<String>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Deserialize)]
pub struct UpdateProviderRequest {
    pub kind: Option<String>,
    pub models: Option<Vec<ModelConfigDto>>,
    pub active_model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Deserialize)]
pub struct SwitchProviderRequest {
    pub name: String,
    pub model_id: Option<String>,
}

fn provider_info_from_snapshot(snapshot: &ProviderInfoSnapshot) -> ProviderInfo {
    ProviderInfo {
        name: snapshot.name.clone(),
        model: snapshot.model.clone(),
        connected: snapshot.connected,
    }
}

fn runtime_worker_error_response(
    error: RuntimeWorkerError,
) -> (StatusCode, Json<serde_json::Value>) {
    (error.status, Json(serde_json::json!({ "error": error.message })))
}

fn trace_store_error_response(error: LlmTraceStoreError) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": error.to_string() })))
}

pub async fn list_traces(
    State(state): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.trace_store.clone();
    match tokio::task::spawn_blocking(move || store.list(100)).await {
        Ok(Ok(items)) => {
            (StatusCode::OK, Json(serde_json::to_value(items).expect("serialize traces")))
        }
        Ok(Err(error)) => trace_store_error_response(error),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

pub async fn get_trace(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.trace_store.clone();
    let missing_id = id.clone();
    match tokio::task::spawn_blocking(move || store.get(&id)).await {
        Ok(Ok(Some(trace))) => {
            (StatusCode::OK, Json(serde_json::to_value(trace).expect("serialize trace")))
        }
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("trace 不存在：{missing_id}") })),
        ),
        Ok(Err(error)) => trace_store_error_response(error),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

pub async fn get_trace_summary(
    State(state): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.trace_store.clone();
    match tokio::task::spawn_blocking(move || store.summary()).await {
        Ok(Ok(summary)) => {
            (StatusCode::OK, Json(serde_json::to_value(summary).expect("serialize trace summary")))
        }
        Ok(Err(error)) => trace_store_error_response(error),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

pub async fn get_providers(State(state): State<SharedState>) -> Json<ProviderInfo> {
    let snapshot = state.provider_info_snapshot.read().expect("lock poisoned");
    Json(provider_info_from_snapshot(&snapshot))
}

pub async fn list_providers(State(state): State<SharedState>) -> Json<Vec<ProviderListItem>> {
    let registry = state.provider_registry_snapshot.read().expect("lock poisoned");
    let active_name = registry.active_provider().map(|p| p.name.clone());
    let items: Vec<ProviderListItem> = registry
        .providers()
        .iter()
        .map(|p| ProviderListItem {
            name: p.name.clone(),
            kind: p.kind.protocol_name().to_string(),
            models: p.models.iter().map(ModelConfigDto::from).collect(),
            active_model: p.active_model.clone(),
            base_url: p.base_url.clone(),
            active: active_name.as_deref() == Some(&p.name),
        })
        .collect();
    Json(items)
}

pub async fn create_provider(
    State(state): State<SharedState>,
    Json(body): Json<CreateProviderRequest>,
) -> impl IntoResponse {
    let kind = match body.kind.as_str() {
        "openai-responses" => ProviderKind::OpenAiResponses,
        "openai-chat-completions" => ProviderKind::OpenAiChatCompletions,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("未知协议：{}", body.kind) })),
            );
        }
    };

    let models: Vec<ModelConfig> = body.models.into_iter().map(ModelConfig::from).collect();
    let result = state
        .worker
        .create_provider(CreateProviderInput {
            name: body.name,
            kind,
            models,
            active_model: body.active_model,
            api_key: body.api_key,
            base_url: body.base_url,
        })
        .await;
    if let Err(error) = result {
        return runtime_worker_error_response(error);
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn update_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateProviderRequest>,
) -> impl IntoResponse {
    let models = body.models.map(|dtos| dtos.into_iter().map(ModelConfig::from).collect());
    let kind = if let Some(kind_str) = &body.kind {
        match kind_str.as_str() {
            "openai-responses" => ProviderKind::OpenAiResponses,
            "openai-chat-completions" => ProviderKind::OpenAiChatCompletions,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": format!("未知协议：{kind_str}") })),
                );
            }
        }
    } else {
        match state
            .worker
            .update_provider(
                name,
                UpdateProviderInput {
                    kind: None,
                    models,
                    active_model: body.active_model,
                    api_key: body.api_key,
                    base_url: body.base_url,
                },
            )
            .await
        {
            Ok(()) => return (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
            Err(error) => return runtime_worker_error_response(error),
        }
    };

    let result = state
        .worker
        .update_provider(
            name,
            UpdateProviderInput {
                kind: Some(kind),
                models,
                active_model: body.active_model,
                api_key: body.api_key,
                base_url: body.base_url,
            },
        )
        .await;
    if let Err(error) = result {
        return runtime_worker_error_response(error);
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err(error) = state.worker.delete_provider(name).await {
        return runtime_worker_error_response(error);
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn switch_provider(
    State(state): State<SharedState>,
    Json(body): Json<SwitchProviderRequest>,
) -> impl IntoResponse {
    let info = match state
        .worker
        .switch_provider(SwitchProviderInput { name: body.name, model_id: body.model_id })
        .await
    {
        Ok(info) => info,
        Err(error) => return runtime_worker_error_response(error),
    };

    (StatusCode::OK, Json(serde_json::json!(provider_info_from_snapshot(&info))))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use agent_core::{Message, Role, ToolCall, ToolResult};
    use agent_runtime::{AgentRuntime, RuntimeEvent, TurnLifecycle};
    use axum::Json;
    use axum::http::StatusCode;
    use llm_trace::{
        LlmTraceListItem, LlmTraceRecord, LlmTraceStatus, LlmTraceStore, LlmTraceSummary,
        SqliteLlmTraceStore,
    };
    use provider_registry::{
        ModelConfig, ModelLimit, ProviderKind, ProviderProfile, ProviderRegistry,
    };
    use session_tape::{SessionTape, TapeEntry};
    use tokio::sync::broadcast;

    use crate::{
        runtime_worker::{self, ProviderInfoSnapshot, RuntimeOwnerState, spawn_runtime_worker},
        sse::SsePayload,
        state::AppState,
    };

    use super::{
        ModelConfigDto, ModelLimitDto, get_providers, get_trace, get_trace_summary, list_providers,
        list_traces,
    };

    fn provider(name: &str, model: &str) -> ProviderProfile {
        ProviderProfile {
            name: name.to_string(),
            kind: ProviderKind::OpenAiResponses,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: model.to_string(),
                display_name: None,
                limit: Some(ModelLimit { context: Some(200_000), output: Some(131_072) }),
                default_temperature: None,
                supports_reasoning: false,
                reasoning_effort: None,
            }],
            active_model: Some(model.to_string()),
        }
    }

    fn shared_state_with_snapshots(
        provider_info: ProviderInfoSnapshot,
        registry: ProviderRegistry,
    ) -> Arc<AppState> {
        let mut runtime = AgentRuntime::new(
            crate::model::ServerModel::bootstrap(),
            builtin_tools::build_tool_registry(),
            agent_core::ModelIdentity::new(
                "local",
                "bootstrap",
                agent_core::ModelDisposition::Balanced,
            ),
        );
        let subscriber = runtime.subscribe();
        let (broadcast_tx, _) = broadcast::channel(16);
        let trace_store: Arc<dyn LlmTraceStore> =
            Arc::new(SqliteLlmTraceStore::in_memory().expect("trace store should init"));
        let provider_registry_snapshot = Arc::new(RwLock::new(registry.clone()));
        let provider_info_snapshot = Arc::new(RwLock::new(provider_info.clone()));
        let worker = spawn_runtime_worker(RuntimeOwnerState {
            runtime,
            subscriber,
            session_path: std::path::PathBuf::from("/tmp/session.jsonl"),
            registry: registry.clone(),
            store_path: std::path::PathBuf::from("/tmp/providers.json"),
            trace_store: trace_store.clone(),
            broadcast_tx: broadcast_tx.clone(),
            provider_registry_snapshot: provider_registry_snapshot.clone(),
            provider_info_snapshot: provider_info_snapshot.clone(),
        });

        let state = Arc::new(AppState {
            worker,
            broadcast_tx,
            provider_registry_snapshot,
            provider_info_snapshot,
            trace_store,
        });
        *state.provider_info_snapshot.write().expect("lock poisoned") = provider_info;
        *state.provider_registry_snapshot.write().expect("lock poisoned") = registry;
        state
    }

    #[test]
    fn rebuild_turn_history_from_tape_restores_completed_turns() {
        let mut tape = SessionTape::new();
        let turn_id = "turn-1";
        let user = Message::new(Role::User, "你好");
        let assistant = Message::new(Role::Assistant, "已完成");
        let call = ToolCall::new("read").with_invocation_id("call-1");
        let result = ToolResult::from_call(&call, "内容");

        tape.append_entry(TapeEntry::message(&user).with_run_id(turn_id));
        tape.append_entry(TapeEntry::thinking("思考中").with_run_id(turn_id));
        tape.append_entry(TapeEntry::tool_call(&call).with_run_id(turn_id));
        tape.append_entry(TapeEntry::tool_result(&result).with_run_id(turn_id));
        tape.append_entry(TapeEntry::message(&assistant).with_run_id(turn_id));

        let turns = runtime_worker::rebuild_turn_history_from_tape(&tape);

        assert_eq!(turns.len(), 1);
        let turn = &turns[0];
        assert_eq!(turn.turn_id, turn_id);
        assert_eq!(turn.user_message, "你好");
        assert_eq!(turn.assistant_message.as_deref(), Some("已完成"));
        assert_eq!(turn.thinking.as_deref(), Some("思考中"));
        assert_eq!(turn.tool_invocations.len(), 1);
        assert_eq!(turn.blocks.len(), 3);
    }

    #[test]
    fn rebuild_turn_history_from_tape_restores_legacy_turn_record() {
        let mut tape = SessionTape::new();
        let legacy_turn = TurnLifecycle {
            turn_id: "legacy-turn-1".to_string(),
            started_at_ms: 1000,
            finished_at_ms: 2000,
            source_entry_ids: vec![1, 2],
            user_message: "旧问题".to_string(),
            blocks: vec![agent_runtime::TurnBlock::Assistant { content: "旧回答".to_string() }],
            assistant_message: Some("旧回答".to_string()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };
        tape.append_entry(TapeEntry::event(
            "turn_record",
            Some(serde_json::to_value(&legacy_turn).expect("legacy turn should serialize")),
        ));

        let turns = runtime_worker::rebuild_turn_history_from_tape(&tape);

        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0], legacy_turn);
    }

    #[test]
    fn model_config_dto_round_trip_preserves_limit() {
        let dto = ModelConfigDto {
            id: "gpt-4.1".into(),
            display_name: Some("GPT-4.1".into()),
            limit: Some(ModelLimitDto { context: Some(200_000), output: Some(131_072) }),
            default_temperature: Some(0.2),
            supports_reasoning: true,
            reasoning_effort: Some("medium".into()),
        };

        let model = ModelConfig::from(dto.clone());
        assert_eq!(model.limit, Some(ModelLimit { context: Some(200_000), output: Some(131_072) }));

        let round_trip = ModelConfigDto::from(&model);
        assert_eq!(round_trip.limit, dto.limit);
    }

    #[test]
    fn broadcast_runtime_events_forwards_context_compression_and_turn() {
        let (broadcast_tx, mut broadcast_rx) = broadcast::channel(8);
        let turn = TurnLifecycle {
            turn_id: "turn-1".to_string(),
            started_at_ms: 1000,
            finished_at_ms: 2000,
            source_entry_ids: vec![1, 2, 3],
            user_message: "你好".to_string(),
            blocks: vec![agent_runtime::TurnBlock::Assistant { content: "已完成".to_string() }],
            assistant_message: Some("已完成".to_string()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: Some("模型执行失败：上下文过长".to_string()),
        };

        let forwarded_turn = runtime_worker::broadcast_runtime_events(
            vec![
                RuntimeEvent::ContextCompressed {
                    summary: "摘要：已压缩历史上下文".to_string()
                },
                RuntimeEvent::TurnLifecycle { turn: turn.clone() },
            ],
            &broadcast_tx,
        );

        assert_eq!(forwarded_turn, Some(turn));
        assert!(matches!(
            broadcast_rx.try_recv().expect("应先转发压缩事件"),
            SsePayload::ContextCompressed { summary } if summary == "摘要：已压缩历史上下文"
        ));
    }

    #[tokio::test]
    async fn get_providers_reads_provider_info_snapshot() {
        let state = shared_state_with_snapshots(
            ProviderInfoSnapshot {
                name: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                connected: true,
            },
            ProviderRegistry::default(),
        );

        let Json(info) = get_providers(axum::extract::State(state)).await;

        assert_eq!(info.name, "openai");
        assert_eq!(info.model, "gpt-4.1");
        assert!(info.connected);
    }

    #[tokio::test]
    async fn list_providers_reads_registry_snapshot() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("alpha", "gpt-4.1-mini"));
        registry.upsert(provider("beta", "gpt-4.1"));
        registry.set_active("beta").expect("beta should exist");
        let state = shared_state_with_snapshots(
            ProviderInfoSnapshot {
                name: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                connected: true,
            },
            registry,
        );

        let Json(items) = list_providers(axum::extract::State(state)).await;

        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.name == "beta" && item.active));
        assert!(items.iter().any(|item| item.name == "alpha" && !item.active));
    }

    #[tokio::test]
    async fn list_traces_reads_trace_store() {
        let state = shared_state_with_snapshots(
            ProviderInfoSnapshot {
                name: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                connected: true,
            },
            ProviderRegistry::default(),
        );
        state
            .trace_store
            .record(&LlmTraceRecord {
                id: "trace-1".to_string(),
                turn_id: "turn-1".to_string(),
                run_id: "turn-1".to_string(),
                request_kind: "completion".to_string(),
                step_index: 0,
                provider: "openai".to_string(),
                protocol: "openai-responses".to_string(),
                model: "gpt-5.4".to_string(),
                base_url: "https://example.com".to_string(),
                endpoint_path: "/responses".to_string(),
                streaming: true,
                started_at_ms: 10,
                finished_at_ms: Some(20),
                duration_ms: Some(10),
                status_code: Some(200),
                status: LlmTraceStatus::Succeeded,
                stop_reason: Some("stop".to_string()),
                error: None,
                checkpoint_in: None,
                checkpoint_out: Some("resp_1".to_string()),
                request_summary: serde_json::json!({"conversation_items": 1}),
                provider_request: serde_json::json!({"model": "gpt-5.4"}),
                response_summary: serde_json::json!({"assistant_text": "ok"}),
                response_body: Some("ok".to_string()),
                input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
            })
            .expect("trace should persist");

        let (status, Json(value)) = list_traces(axum::extract::State(state)).await;
        let items: Vec<LlmTraceListItem> = serde_json::from_value(value).expect("decode traces");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "trace-1");
        assert_eq!(items[0].user_message, None);
    }

    #[tokio::test]
    async fn trace_detail_and_summary_routes_read_trace_store() {
        let state = shared_state_with_snapshots(
            ProviderInfoSnapshot {
                name: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                connected: true,
            },
            ProviderRegistry::default(),
        );
        let record = LlmTraceRecord {
            id: "trace-2".to_string(),
            turn_id: "turn-2".to_string(),
            run_id: "turn-2".to_string(),
            request_kind: "completion".to_string(),
            step_index: 1,
            provider: "openai".to_string(),
            protocol: "openai-chat-completions".to_string(),
            model: "gpt-5.4".to_string(),
            base_url: "https://example.com".to_string(),
            endpoint_path: "/chat/completions".to_string(),
            streaming: true,
            started_at_ms: 100,
            finished_at_ms: Some(160),
            duration_ms: Some(60),
            status_code: Some(500),
            status: LlmTraceStatus::Failed,
            stop_reason: None,
            error: Some("boom".to_string()),
            checkpoint_in: None,
            checkpoint_out: None,
            request_summary: serde_json::json!({"conversation_items": 2}),
            provider_request: serde_json::json!({"model": "gpt-5.4"}),
            response_summary: serde_json::json!({"error": "boom"}),
            response_body: None,
            input_tokens: Some(10),
            output_tokens: Some(0),
            total_tokens: Some(10),
        };
        state.trace_store.record(&record).expect("trace should persist");

        let (detail_status, Json(detail_value)) = get_trace(
            axum::extract::State(state.clone()),
            axum::extract::Path("trace-2".to_string()),
        )
        .await;
        let detail: LlmTraceRecord =
            serde_json::from_value(detail_value).expect("decode trace detail");
        assert_eq!(detail_status, StatusCode::OK);
        assert_eq!(detail.id, "trace-2");

        let (summary_status, Json(summary_value)) =
            get_trace_summary(axum::extract::State(state)).await;
        let summary: LlmTraceSummary =
            serde_json::from_value(summary_value).expect("decode trace summary");
        assert_eq!(summary_status, StatusCode::OK);
        assert_eq!(summary.total_requests, 1);
        assert_eq!(summary.failed_requests, 1);
    }
}

pub async fn get_session_info(State(state): State<SharedState>) -> impl IntoResponse {
    match state.worker.get_session_info().await {
        Ok(stats) => (StatusCode::OK, Json(serde_json::to_value(stats).expect("serialize stats"))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn create_handoff(
    State(state): State<SharedState>,
    Json(body): Json<HandoffRequest>,
) -> impl IntoResponse {
    match state.worker.create_handoff(body.name, body.summary).await {
        Ok(anchor_entry_id) => {
            (StatusCode::OK, Json(serde_json::json!({ "anchor_entry_id": anchor_entry_id })))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn get_history(State(state): State<SharedState>) -> impl IntoResponse {
    match state.worker.get_history().await {
        Ok(turns) => {
            (StatusCode::OK, Json(serde_json::to_value(turns).expect("serialize history")))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

/// Global SSE endpoint — client connects once, receives all events.
pub async fn events(State(state): State<SharedState>) -> impl IntoResponse {
    let rx = state.broadcast_tx.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(payload) => Some(payload.into_axum_event()),
        Err(_) => None, // lagged — skip missed events
    });

    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

/// Fire-and-forget turn submission. Events arrive via the global SSE stream.
pub async fn submit_turn(
    State(state): State<SharedState>,
    Json(body): Json<TurnRequest>,
) -> impl IntoResponse {
    let _ = state.broadcast_tx.send(SsePayload::Status(TurnStatus::Waiting));
    if let Err(error) = state.worker.submit_turn(body.prompt) {
        return runtime_worker_error_response(error);
    }

    (StatusCode::ACCEPTED, Json(serde_json::json!({ "ok": true })))
}
