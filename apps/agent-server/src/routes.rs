use agent_store::{LlmTraceStore, LlmTraceStoreError};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use provider_registry::{ModelConfig, ModelLimit, ProviderKind};

use crate::{
    session_manager::{
        CreateProviderInput, ProviderInfoSnapshot, RuntimeWorkerError, SwitchProviderInput,
        UpdateProviderInput,
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
pub struct CancelTurnRequest {
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct HandoffRequest {
    pub name: String,
    pub summary: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AutoCompressRequest {
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    pub session_id: Option<String>,
    pub before_turn_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct TraceListQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
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

fn json_response<T: Serialize>(
    status: StatusCode,
    payload: T,
) -> (StatusCode, Json<serde_json::Value>) {
    match serde_json::to_value(payload) {
        Ok(value) => (status, Json(value)),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("response serialization failed: {error}")
            })),
        ),
    }
}

/// Resolve session_id: use provided, or fall back to first session from DB
fn resolve_session_id(
    state: &crate::state::AppState,
    session_id: Option<String>,
) -> Option<String> {
    if let Some(id) = session_id {
        return Some(id);
    }
    // Fall back to first session
    state.store.list_sessions().ok().and_then(|sessions| sessions.first().map(|s| s.id.clone()))
}

// ── Session management ─────────────────────────────────────────

pub async fn list_sessions(
    State(state): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.store.list_sessions() {
        Ok(sessions) => json_response(StatusCode::OK, sessions),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
        }
    }
}

pub async fn create_session(
    State(state): State<SharedState>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    match state.session_manager.create_session(body.title).await {
        Ok(record) => json_response(StatusCode::CREATED, record),
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
    Query(query): Query<TraceListQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let page_size = query.page_size.unwrap_or(12).clamp(1, 50);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * page_size;
    let store = state.store.clone();
    match tokio::task::spawn_blocking(move || store.list_page(page_size, offset)).await {
        Ok(Ok(result)) => json_response(StatusCode::OK, result),
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
        Ok(Ok(Some(trace))) => json_response(StatusCode::OK, trace),
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
        Ok(Ok(summary)) => json_response(StatusCode::OK, summary),
        Ok(Err(error)) => trace_store_error_response(error),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

// ── Provider endpoints ─────────────────────────────────────────

pub async fn get_providers(State(state): State<SharedState>) -> Json<ProviderInfo> {
    let snapshot = crate::session_manager::read_lock(&state.provider_info_snapshot);
    Json(provider_info_from_snapshot(&snapshot))
}

pub async fn list_providers(State(state): State<SharedState>) -> Json<Vec<ProviderListItem>> {
    let registry = crate::session_manager::read_lock(&state.provider_registry_snapshot);
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
        Ok(stats) => json_response(StatusCode::OK, stats),
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

pub async fn auto_compress_session(
    State(state): State<SharedState>,
    Json(body): Json<AutoCompressRequest>,
) -> impl IntoResponse {
    let Some(session_id) = resolve_session_id(&state, body.session_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no session available" })),
        );
    };
    match state.session_manager.auto_compress_session(session_id).await {
        Ok(compressed) => (StatusCode::OK, Json(serde_json::json!({ "compressed": compressed }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn get_history(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(session_id) = resolve_session_id(&state, query.session_id) else {
        return json_response(StatusCode::OK, Vec::<()>::new());
    };
    match state.session_manager.get_history(session_id).await {
        Ok(turns) => {
            let limit = query.limit.unwrap_or(50).clamp(1, 200);
            let end_exclusive = query
                .before_turn_id
                .as_ref()
                .and_then(|turn_id| turns.iter().position(|turn| &turn.turn_id == turn_id))
                .unwrap_or(turns.len());
            let start = end_exclusive.saturating_sub(limit);
            let page = turns[start..end_exclusive].to_vec();
            let has_more = start > 0;
            let next_before_turn_id = page.first().map(|turn| turn.turn_id.clone());
            let payload = serde_json::json!({
                "turns": page,
                "has_more": has_more,
                "next_before_turn_id": next_before_turn_id,
            });
            (StatusCode::OK, Json(payload))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub async fn get_current_turn(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(session_id) = resolve_session_id(&state, query.session_id) else {
        return json_response(StatusCode::OK, Option::<()>::None);
    };
    match state.session_manager.get_current_turn(session_id).await {
        Ok(current) => json_response(StatusCode::OK, current),
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

    match state.session_manager.get_session_info(session_id.clone()).await {
        Ok(stats) => {
            if stats.pressure_ratio.is_some_and(|ratio| ratio >= agent_prompts::AUTO_COMPRESSION_THRESHOLD)
                && let Err(error) = state.session_manager.auto_compress_session(session_id.clone()).await
            {
                return runtime_worker_error_response(error);
            }
        }
        Err(error) => return runtime_worker_error_response(error),
    }

    let _ = state
        .broadcast_tx
        .send(SsePayload::Status { session_id: session_id.clone(), status: TurnStatus::Waiting });
    if let Err(error) = state.session_manager.submit_turn(session_id, body.prompt) {
        return runtime_worker_error_response(error);
    }

    (StatusCode::ACCEPTED, Json(serde_json::json!({ "ok": true })))
}

pub async fn cancel_turn(
    State(state): State<SharedState>,
    Json(body): Json<CancelTurnRequest>,
) -> impl IntoResponse {
    let Some(session_id) = resolve_session_id(&state, body.session_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no session available" })),
        );
    };

    match state.session_manager.cancel_turn(session_id).await {
        Ok(cancelled) => (StatusCode::OK, Json(serde_json::json!({ "cancelled": cancelled }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::{CancelTurnRequest, ModelConfigDto, ModelLimitDto, json_response};
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

    #[test]
    fn json_response_serializes_payload() {
        let (status, body) = json_response(StatusCode::CREATED, serde_json::json!({ "ok": true }));

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body.0, serde_json::json!({ "ok": true }));
    }

    #[test]
    fn cancel_turn_request_deserializes_session_id() {
        let parsed: CancelTurnRequest = serde_json::from_value(serde_json::json!({
            "session_id": "session-1"
        }))
        .expect("cancel turn request should deserialize");

        assert_eq!(parsed.session_id.as_deref(), Some("session-1"));
    }
}
