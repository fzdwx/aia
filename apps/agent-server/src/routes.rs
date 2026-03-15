use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{KeepAlive, Sse},
    },
};
use aia_store::{LlmTraceStore, LlmTraceStoreError};
use serde::{Deserialize, Serialize};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use provider_registry::{ModelConfig, ModelLimit, ProviderKind};

use crate::{
    session_manager::{
        CreateProviderInput, ProviderInfoSnapshot, RuntimeWorkerError,
        SwitchProviderInput, UpdateProviderInput,
    },
    sse::{SsePayload, TurnStatus},
    state::SharedState,
};

#[derive(Deserialize)]
pub struct TurnRequest {
    pub prompt: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct HandoffRequest {
    pub name: String,
    pub summary: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    pub session_id: Option<String>,
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

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
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

/// Resolve session_id: use provided, or fall back to first session from DB
fn resolve_session_id(state: &crate::state::AppState, session_id: Option<String>) -> Option<String> {
    if let Some(id) = session_id {
        return Some(id);
    }
    // Fall back to first session
    state
        .store
        .list_sessions()
        .ok()
        .and_then(|sessions| sessions.first().map(|s| s.id.clone()))
}

// ── Session management ─────────────────────────────────────────

pub async fn list_sessions(
    State(state): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.store.list_sessions() {
        Ok(sessions) => (
            StatusCode::OK,
            Json(serde_json::to_value(sessions).expect("serialize sessions")),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn create_session(
    State(state): State<SharedState>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    match state.session_manager.create_session(body.title).await {
        Ok(record) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(record).expect("serialize session")),
        ),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn delete_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.session_manager.delete_session(id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

// ── Trace endpoints (unchanged) ────────────────────────────────

pub async fn list_traces(
    State(state): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.clone();
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
    let store = state.store.clone();
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
    let store = state.store.clone();
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

// ── Provider endpoints ─────────────────────────────────────────

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
        .session_manager
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
            .session_manager
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
        .session_manager
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
    if let Err(error) = state.session_manager.delete_provider(name).await {
        return runtime_worker_error_response(error);
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn switch_provider(
    State(state): State<SharedState>,
    Json(body): Json<SwitchProviderRequest>,
) -> impl IntoResponse {
    let info = match state
        .session_manager
        .switch_provider(SwitchProviderInput { name: body.name, model_id: body.model_id })
        .await
    {
        Ok(info) => info,
        Err(error) => return runtime_worker_error_response(error),
    };

    (StatusCode::OK, Json(serde_json::json!(provider_info_from_snapshot(&info))))
}

// ── Session-scoped endpoints ───────────────────────────────────

pub async fn get_session_info(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let Some(session_id) = resolve_session_id(&state, query.session_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no session available" })),
        );
    };
    match state.session_manager.get_session_info(session_id).await {
        Ok(stats) => (StatusCode::OK, Json(serde_json::to_value(stats).expect("serialize stats"))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn create_handoff(
    State(state): State<SharedState>,
    Json(body): Json<HandoffRequest>,
) -> impl IntoResponse {
    let Some(session_id) = resolve_session_id(&state, body.session_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no session available" })),
        );
    };
    match state.session_manager.create_handoff(session_id, body.name, body.summary).await {
        Ok(anchor_entry_id) => {
            (StatusCode::OK, Json(serde_json::json!({ "anchor_entry_id": anchor_entry_id })))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn get_history(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(session_id) = resolve_session_id(&state, query.session_id) else {
        return (StatusCode::OK, Json(serde_json::to_value(Vec::<()>::new()).expect("empty")));
    };
    match state.session_manager.get_history(session_id).await {
        Ok(turns) => (StatusCode::OK, Json(serde_json::to_value(turns).expect("serialize history"))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn get_current_turn(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(session_id) = resolve_session_id(&state, query.session_id) else {
        return (StatusCode::OK, Json(serde_json::to_value(Option::<()>::None).expect("null")));
    };
    match state.session_manager.get_current_turn(session_id).await {
        Ok(current) => {
            (StatusCode::OK, Json(serde_json::to_value(current).expect("serialize current turn")))
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
    let Some(session_id) = resolve_session_id(&state, body.session_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no session available" })),
        );
    };
    let _ = state.broadcast_tx.send(SsePayload::Status {
        session_id: session_id.clone(),
        status: TurnStatus::Waiting,
    });
    if let Err(error) = state.session_manager.submit_turn(session_id, body.prompt) {
        return runtime_worker_error_response(error);
    }

    (StatusCode::ACCEPTED, Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::{ModelConfigDto, ModelLimitDto};
    use provider_registry::{ModelConfig, ModelLimit};

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
}
