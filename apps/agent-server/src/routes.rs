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
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    sse::{SsePayload, TurnStatus},
    state::{AppState, SharedState},
};

#[derive(Deserialize)]
pub struct TurnRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

fn provider_info_from_runtime(state: &AppState) -> ProviderInfo {
    let identity = state.runtime.model_identity();
    ProviderInfo { name: identity.provider.clone(), model: identity.name.clone(), connected: true }
}

fn prepare_runtime_sync(
    registry: &provider_registry::ProviderRegistry,
) -> Result<(ProviderInfo, agent_core::ModelIdentity, ServerModel, SessionProviderBinding), String>
{
    let selection = registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap);

    let (identity, model) = build_model_from_selection(selection).map_err(|e| e.to_string())?;

    let binding = match registry.active_provider() {
        Some(profile) => SessionProviderBinding::Provider {
            name: profile.name.clone(),
            model: profile.active_model_id().unwrap_or("").to_string(),
            base_url: profile.base_url.clone(),
            protocol: profile.kind.protocol_name().to_string(),
        },
        None => SessionProviderBinding::Bootstrap,
    };

    let info = ProviderInfo {
        name: identity.provider.clone(),
        model: identity.name.clone(),
        connected: true,
    };

    Ok((info, identity, model, binding))
}

fn sync_runtime_to_registry(
    state: &mut AppState,
    candidate_registry: provider_registry::ProviderRegistry,
) -> Result<ProviderInfo, String> {
    let (info, identity, model, binding) = prepare_runtime_sync(&candidate_registry)?;
    let mut candidate_tape = state.runtime.tape().clone();
    candidate_tape.bind_provider(binding.clone());

    candidate_registry
        .save(&state.store_path)
        .map_err(|e| format!("provider registry save failed: {e}"))?;
    candidate_tape
        .save_jsonl(&state.session_path)
        .map_err(|e| format!("session save failed: {e}"))?;

    state.registry = candidate_registry;
    state.runtime.replace_model(model, identity);
    *state.runtime.tape_mut() = candidate_tape;
    Ok(info)
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
    Json(provider_info_from_runtime(&s))
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

    let mut candidate_registry = s.registry.clone();
    candidate_registry.upsert(profile);
    if let Err(e) = sync_runtime_to_registry(&mut s, candidate_registry) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e })));
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

    let mut candidate_registry = s.registry.clone();
    candidate_registry.upsert(updated);
    if let Err(e) = sync_runtime_to_registry(&mut s, candidate_registry) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e })));
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut s = state.lock().expect("lock poisoned");

    let mut candidate_registry = s.registry.clone();
    if let Err(e) = candidate_registry.remove(&name) {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e.to_string() })));
    }

    if let Err(e) = sync_runtime_to_registry(&mut s, candidate_registry) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e })));
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
    }

    let mut candidate_registry = s.registry.clone();
    candidate_registry.upsert(profile);
    if let Err(e) = candidate_registry.set_active(&body.name) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() })));
    }
    let info = match sync_runtime_to_registry(&mut s, candidate_registry) {
        Ok(info) => info,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e })));
        }
    };

    (StatusCode::OK, Json(serde_json::json!(info)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agent_core::{ModelDisposition, ModelIdentity};
    use agent_runtime::AgentRuntime;
    use builtin_tools::build_tool_registry;
    use provider_registry::{ModelConfig, ProviderKind, ProviderProfile, ProviderRegistry};
    use session_tape::SessionProviderBinding;
    use tokio::sync::broadcast;

    use crate::{model::BootstrapModel, state::AppState};

    use super::{prepare_runtime_sync, sync_runtime_to_registry};

    fn provider(name: &str, model: &str) -> ProviderProfile {
        ProviderProfile {
            name: name.to_string(),
            kind: ProviderKind::OpenAiResponses,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: model.to_string(),
                display_name: None,
                context_window: None,
                default_temperature: None,
                supports_reasoning: false,
                reasoning_effort: None,
            }],
            active_model: Some(model.to_string()),
        }
    }

    #[test]
    fn sync_runtime_to_active_provider_tracks_registry_after_active_delete() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("first", "gpt-4.1-mini"));
        registry.upsert(provider("second", "gpt-4.1"));
        registry.set_active("first").expect("first provider should exist");

        let runtime = AgentRuntime::new(
            crate::model::ServerModel::Bootstrap(BootstrapModel),
            build_tool_registry(),
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
        );
        let (broadcast_tx, _) = broadcast::channel(16);
        let mut state = AppState {
            runtime,
            subscriber: 1,
            session_path: PathBuf::from("/tmp/session.jsonl"),
            registry,
            store_path: PathBuf::from("/tmp/providers.json"),
            broadcast_tx,
        };

        state.registry.remove("first").expect("active provider should be removable");
        let candidate_registry = state.registry.clone();

        let info = sync_runtime_to_registry(&mut state, candidate_registry)
            .expect("runtime sync should follow the new active provider");

        assert_eq!(info.name, "openai");
        assert_eq!(info.model, "gpt-4.1");
        let binding = state.runtime.tape().latest_provider_binding();
        assert_eq!(
            binding,
            Some(SessionProviderBinding::Provider {
                name: "second".to_string(),
                model: "gpt-4.1".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                protocol: "openai-responses".to_string(),
            })
        );
    }

    #[test]
    fn sync_runtime_to_active_provider_falls_back_to_bootstrap_when_empty() {
        let runtime = AgentRuntime::new(
            crate::model::ServerModel::Bootstrap(BootstrapModel),
            build_tool_registry(),
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
        );
        let (broadcast_tx, _) = broadcast::channel(16);
        let mut state = AppState {
            runtime,
            subscriber: 1,
            session_path: PathBuf::from("/tmp/session.jsonl"),
            registry: ProviderRegistry::default(),
            store_path: PathBuf::from("/tmp/providers.json"),
            broadcast_tx,
        };
        let candidate_registry = state.registry.clone();

        let info = sync_runtime_to_registry(&mut state, candidate_registry)
            .expect("runtime sync should support bootstrap fallback");

        assert_eq!(info.name, "local");
        assert_eq!(info.model, "bootstrap");
        assert_eq!(
            state.runtime.tape().latest_provider_binding(),
            Some(SessionProviderBinding::Bootstrap)
        );
    }

    #[test]
    fn prepare_runtime_sync_failure_does_not_mutate_existing_state() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("stable", "gpt-4.1-mini"));

        let runtime = AgentRuntime::new(
            crate::model::ServerModel::Bootstrap(BootstrapModel),
            build_tool_registry(),
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
        );
        let (broadcast_tx, _) = broadcast::channel(16);
        let mut state = AppState {
            runtime,
            subscriber: 1,
            session_path: PathBuf::from("/tmp/session.jsonl"),
            registry: registry.clone(),
            store_path: PathBuf::from("/tmp/providers.json"),
            broadcast_tx,
        };

        sync_runtime_to_registry(&mut state, registry).expect("initial sync should succeed");

        let mut invalid_registry = state.registry.clone();
        invalid_registry.upsert(ProviderProfile {
            name: "stable".to_string(),
            kind: ProviderKind::OpenAiResponses,
            base_url: "".to_string(),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: "gpt-4.1-mini".to_string(),
                display_name: None,
                context_window: None,
                default_temperature: None,
                supports_reasoning: false,
                reasoning_effort: None,
            }],
            active_model: Some("gpt-4.1-mini".to_string()),
        });

        let before_identity = state.runtime.model_identity().clone();
        let before_binding = state.runtime.tape().latest_provider_binding();
        assert!(prepare_runtime_sync(&invalid_registry).is_err());

        assert_eq!(state.runtime.model_identity(), &before_identity);
        assert_eq!(state.runtime.tape().latest_provider_binding(), before_binding);
        assert_eq!(state.registry.active_provider().map(|p| p.name.as_str()), Some("stable"));
        assert_eq!(
            state.registry.active_provider().map(|p| p.base_url.as_str()),
            Some("https://api.openai.com/v1")
        );
    }

    #[test]
    fn sync_runtime_to_registry_failure_does_not_mutate_when_registry_save_fails() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("stable", "gpt-4.1-mini"));

        let runtime = AgentRuntime::new(
            crate::model::ServerModel::Bootstrap(BootstrapModel),
            build_tool_registry(),
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
        );
        let (broadcast_tx, _) = broadcast::channel(16);
        let mut state = AppState {
            runtime,
            subscriber: 1,
            session_path: PathBuf::from("/tmp/session.jsonl"),
            registry: ProviderRegistry::default(),
            store_path: PathBuf::from("/proc/aia/providers.json"),
            broadcast_tx,
        };

        let before_identity = state.runtime.model_identity().clone();
        let before_binding = state.runtime.tape().latest_provider_binding();

        let error =
            sync_runtime_to_registry(&mut state, registry).expect_err("registry save should fail");

        assert!(error.contains("provider registry save failed"));
        assert_eq!(state.runtime.model_identity(), &before_identity);
        assert_eq!(state.runtime.tape().latest_provider_binding(), before_binding);
        assert!(state.registry.providers().is_empty());
    }

    #[test]
    fn sync_runtime_to_registry_failure_does_not_mutate_when_session_save_fails() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("stable", "gpt-4.1-mini"));

        let runtime = AgentRuntime::new(
            crate::model::ServerModel::Bootstrap(BootstrapModel),
            build_tool_registry(),
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
        );
        let (broadcast_tx, _) = broadcast::channel(16);
        let mut state = AppState {
            runtime,
            subscriber: 1,
            session_path: PathBuf::from("/proc/aia/session.jsonl"),
            registry: ProviderRegistry::default(),
            store_path: PathBuf::from("/tmp/providers.json"),
            broadcast_tx,
        };

        let before_identity = state.runtime.model_identity().clone();
        let before_binding = state.runtime.tape().latest_provider_binding();

        let error =
            sync_runtime_to_registry(&mut state, registry).expect_err("session save should fail");

        assert!(error.contains("session save failed"));
        assert_eq!(state.runtime.model_identity(), &before_identity);
        assert_eq!(state.runtime.tape().latest_provider_binding(), before_binding);
        assert!(state.registry.providers().is_empty());
    }
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
