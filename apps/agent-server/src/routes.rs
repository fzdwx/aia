use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use agent_core::StreamEvent;
use agent_runtime::{RuntimeEvent, TurnLifecycle};
use provider_registry::{ModelConfig, ProviderKind};
use session_tape::SessionProviderBinding;

use crate::{
    model::{ProviderLaunchChoice, build_model_from_selection},
    sse::{SsePayload, TurnStatus},
    state::SharedState,
};

#[derive(Deserialize)]
pub struct TurnRequest {
    pub prompt: String,
}

#[derive(Serialize)]
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

#[derive(Serialize, Deserialize, Clone)]
pub struct ModelConfigDto {
    pub id: String,
    pub display_name: Option<String>,
    pub context_window: Option<u32>,
    pub default_temperature: Option<f32>,
    #[serde(default)]
    pub supports_reasoning: bool,
    pub reasoning_effort: Option<String>,
}

impl From<&ModelConfig> for ModelConfigDto {
    fn from(m: &ModelConfig) -> Self {
        Self {
            id: m.id.clone(),
            display_name: m.display_name.clone(),
            context_window: m.context_window,
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
            context_window: dto.context_window,
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

#[derive(Serialize)]
struct TurnAccepted {
    ok: bool,
}

pub async fn get_providers(State(state): State<SharedState>) -> Json<ProviderInfo> {
    let s = state.lock().expect("lock poisoned");
    let identity = s.runtime.model_identity();
    Json(ProviderInfo {
        name: identity.provider.clone(),
        model: identity.name.clone(),
        connected: true,
    })
}

pub async fn list_providers(State(state): State<SharedState>) -> Json<Vec<ProviderListItem>> {
    let s = state.lock().expect("lock poisoned");
    let active_name = s.registry.active_provider().map(|p| p.name.clone());
    let items: Vec<ProviderListItem> = s
        .registry
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
    let mut s = state.lock().expect("lock poisoned");
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
    let active_model = body.active_model.or_else(|| models.first().map(|m| m.id.clone()));

    let profile = provider_registry::ProviderProfile {
        name: body.name,
        kind,
        base_url: body.base_url,
        api_key: body.api_key,
        models,
        active_model,
    };

    s.registry.upsert(profile);
    if let Err(e) = s.registry.save(&s.store_path) {
        eprintln!("provider registry save failed: {e}");
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn update_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateProviderRequest>,
) -> impl IntoResponse {
    let mut s = state.lock().expect("lock poisoned");

    let profile = match s.registry.providers().iter().find(|p| p.name == name) {
        Some(p) => p.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("provider 不存在：{name}") })),
            );
        }
    };

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
        profile.kind.clone()
    };

    let models = body
        .models
        .map(|dtos| dtos.into_iter().map(ModelConfig::from).collect())
        .unwrap_or(profile.models.clone());

    let active_model = body.active_model.or(profile.active_model.clone());

    let updated = provider_registry::ProviderProfile {
        name: name.clone(),
        kind,
        base_url: body.base_url.unwrap_or(profile.base_url),
        api_key: body.api_key.unwrap_or(profile.api_key),
        models,
        active_model,
    };

    s.registry.upsert(updated);
    if let Err(e) = s.registry.save(&s.store_path) {
        eprintln!("provider registry save failed: {e}");
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut s = state.lock().expect("lock poisoned");

    if let Err(e) = s.registry.remove(&name) {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e.to_string() })));
    }

    if let Err(e) = s.registry.save(&s.store_path) {
        eprintln!("provider registry save failed: {e}");
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn switch_provider(
    State(state): State<SharedState>,
    Json(body): Json<SwitchProviderRequest>,
) -> impl IntoResponse {
    let mut s = state.lock().expect("lock poisoned");

    let mut profile = match s.registry.providers().iter().find(|p| p.name == body.name) {
        Some(p) => p.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("provider 不存在：{}", body.name) })),
            );
        }
    };

    // If model_id provided, set it as active_model on the profile
    if let Some(model_id) = &body.model_id {
        if !profile.has_model(model_id) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("模型不存在：{model_id}") })),
            );
        }
        profile.active_model = Some(model_id.clone());
        // Persist the active_model change
        s.registry.upsert(profile.clone());
    }

    let selection = ProviderLaunchChoice::OpenAi(profile.clone());
    let (identity, model) = match build_model_from_selection(selection) {
        Ok(pair) => pair,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    s.runtime.replace_model(model, identity.clone());

    if let Err(e) = s.registry.set_active(&body.name) {
        eprintln!("set active failed: {e}");
    }
    if let Err(e) = s.registry.save(&s.store_path) {
        eprintln!("provider registry save failed: {e}");
    }

    let model_id = profile.active_model_id().unwrap_or("").to_string();

    s.runtime.tape_mut().bind_provider(SessionProviderBinding::Provider {
        name: profile.name,
        model: model_id.clone(),
        base_url: profile.base_url,
        protocol: profile.kind.protocol_name().to_string(),
    });

    if let Err(e) = s.runtime.tape().save_jsonl(&s.session_path) {
        eprintln!("session save failed: {e}");
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "name": identity.provider,
            "model": identity.name,
            "connected": true,
        })),
    )
}

pub async fn get_history(State(state): State<SharedState>) -> impl IntoResponse {
    let mut s = state.lock().expect("lock poisoned");
    let sub = s.subscriber;
    let events = s.runtime.collect_events(sub).unwrap_or_default();
    let turns: Vec<TurnLifecycle> = events
        .into_iter()
        .filter_map(|event| match event {
            RuntimeEvent::TurnLifecycle { turn } => Some(turn),
            _ => None,
        })
        .collect();
    Json(turns)
}

/// Global SSE endpoint — client connects once, receives all events.
pub async fn events(
    State(state): State<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = {
        let s = state.lock().expect("lock poisoned");
        s.broadcast_tx.subscribe()
    };

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(payload) => Some(payload.into_axum_event()),
        Err(_) => None, // lagged — skip missed events
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Fire-and-forget turn submission. Events arrive via the global SSE stream.
pub async fn submit_turn(
    State(state): State<SharedState>,
    Json(body): Json<TurnRequest>,
) -> impl IntoResponse {
    let broadcast_tx = {
        let s = state.lock().expect("lock poisoned");
        s.broadcast_tx.clone()
    };

    // Immediately signal "waiting"
    let _ = broadcast_tx.send(SsePayload::Status(TurnStatus::Waiting));

    tokio::task::spawn_blocking(move || {
        let mut current_status = CurrentStatus::Waiting;
        let btx = broadcast_tx.clone();

        let result = {
            let mut s = state.lock().expect("lock poisoned");
            s.runtime.handle_turn_streaming(&body.prompt, |event| {
                // Derive status from event kind
                let new_status = match &event {
                    StreamEvent::ThinkingDelta { .. } => CurrentStatus::Thinking,
                    StreamEvent::TextDelta { .. } => CurrentStatus::Generating,
                    StreamEvent::ToolCallStarted { .. } => CurrentStatus::Working,
                    StreamEvent::ToolOutputDelta { .. } => CurrentStatus::Working,
                    _ => current_status.clone(),
                };

                if new_status != current_status {
                    current_status = new_status.clone();
                    let _ = btx.send(SsePayload::Status(new_status.to_turn_status()));
                }

                let _ = btx.send(SsePayload::Stream(event));
            })
        };

        match result {
            Ok(_) => {
                let mut s = state.lock().expect("lock poisoned");
                let sub = s.subscriber;
                let events = s.runtime.collect_events(sub).unwrap_or_default();
                let turn = events.into_iter().find_map(|event| match event {
                    RuntimeEvent::TurnLifecycle { turn } => Some(turn),
                    _ => None,
                });
                if let Some(turn) = turn {
                    let _ = broadcast_tx.send(SsePayload::TurnCompleted(turn));
                }
                if let Err(e) = s.runtime.tape().save_jsonl(&s.session_path) {
                    eprintln!("session save failed: {e}");
                }
            }
            Err(error) => {
                let _ = broadcast_tx.send(SsePayload::Error(error.to_string()));
            }
        }
    });

    (StatusCode::ACCEPTED, Json(TurnAccepted { ok: true }))
}

#[derive(Clone, PartialEq)]
enum CurrentStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
}

impl CurrentStatus {
    fn to_turn_status(&self) -> TurnStatus {
        match self {
            Self::Waiting => TurnStatus::Waiting,
            Self::Thinking => TurnStatus::Thinking,
            Self::Working => TurnStatus::Working,
            Self::Generating => TurnStatus::Generating,
        }
    }
}
