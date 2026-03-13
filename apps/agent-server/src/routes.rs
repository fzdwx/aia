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

use std::collections::BTreeMap;

use agent_core::{Role, StreamEvent};
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

#[derive(Default)]
struct TurnHistoryBuilder {
    turn_id: String,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    source_entry_ids: Vec<u64>,
    user_message: Option<String>,
    blocks: Vec<agent_runtime::TurnBlock>,
    assistant_message: Option<String>,
    thinking: Option<String>,
    tool_invocations: Vec<agent_runtime::ToolInvocationLifecycle>,
    failure_message: Option<String>,
    pending_tool_calls: BTreeMap<String, agent_core::ToolCall>,
}

impl TurnHistoryBuilder {
    fn new(turn_id: String) -> Self {
        Self { turn_id, ..Self::default() }
    }

    fn push_entry(&mut self, entry: &session_tape::TapeEntry) {
        self.source_entry_ids.push(entry.id);
        let timestamp_ms = parse_iso8601_utc_seconds(&entry.date).unwrap_or(0);
        self.started_at_ms = Some(self.started_at_ms.unwrap_or(timestamp_ms));
        self.finished_at_ms = Some(timestamp_ms);

        if let Some(message) = entry.as_message() {
            match message.role {
                Role::User => {
                    if self.user_message.is_none() {
                        self.user_message = Some(message.content);
                    }
                }
                Role::Assistant => {
                    self.assistant_message = Some(message.content.clone());
                    self.blocks
                        .push(agent_runtime::TurnBlock::Assistant { content: message.content });
                }
                Role::System | Role::Tool => {}
            }
            return;
        }

        if let Some(content) = entry.as_thinking() {
            match &mut self.thinking {
                Some(existing) => existing.push_str(content),
                None => self.thinking = Some(content.to_string()),
            }
            self.blocks.push(agent_runtime::TurnBlock::Thinking { content: content.to_string() });
            return;
        }

        if let Some(call) = entry.as_tool_call() {
            self.pending_tool_calls.insert(call.invocation_id.clone(), call);
            return;
        }

        if let Some(result) = entry.as_tool_result() {
            let call = self.pending_tool_calls.remove(&result.invocation_id).unwrap_or_else(|| {
                agent_core::ToolCall::new(result.tool_name.clone())
                    .with_invocation_id(result.invocation_id.clone())
            });
            let invocation = agent_runtime::ToolInvocationLifecycle {
                call,
                outcome: agent_runtime::ToolInvocationOutcome::Succeeded { result },
            };
            self.blocks
                .push(agent_runtime::TurnBlock::ToolInvocation { invocation: invocation.clone() });
            self.tool_invocations.push(invocation);
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_failed") {
            let message = entry
                .event_data()
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .unwrap_or("turn failed")
                .to_string();
            self.failure_message = Some(message.clone());
            self.blocks.push(agent_runtime::TurnBlock::Failure { message });
        }
    }

    fn finish(self) -> Option<TurnLifecycle> {
        let user_message = self.user_message?;
        Some(TurnLifecycle {
            turn_id: self.turn_id,
            started_at_ms: self.started_at_ms.unwrap_or(0),
            finished_at_ms: self.finished_at_ms.unwrap_or(0),
            source_entry_ids: self.source_entry_ids,
            user_message,
            blocks: self.blocks,
            assistant_message: self.assistant_message,
            thinking: self.thinking,
            tool_invocations: self.tool_invocations,
            failure_message: self.failure_message,
        })
    }
}

fn rebuild_turn_history_from_tape(tape: &session_tape::SessionTape) -> Vec<TurnLifecycle> {
    let mut builders = Vec::<TurnHistoryBuilder>::new();
    let mut by_run_id = BTreeMap::<String, usize>::new();
    let mut legacy_turns = Vec::<TurnLifecycle>::new();

    for entry in tape.entries() {
        if entry.kind == "event" && entry.event_name() == Some("turn_record") {
            if let Some(turn) = parse_legacy_turn_record(entry) {
                legacy_turns.push(turn);
            }
            continue;
        }

        let run_id = entry
            .meta
            .get("run_id")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(run_id) = run_id else {
            continue;
        };

        let index = match by_run_id.get(&run_id) {
            Some(index) => *index,
            None => {
                let index = builders.len();
                builders.push(TurnHistoryBuilder::new(run_id.clone()));
                by_run_id.insert(run_id, index);
                index
            }
        };
        builders[index].push_entry(entry);
    }

    legacy_turns.extend(builders.into_iter().filter_map(TurnHistoryBuilder::finish));
    legacy_turns
        .sort_by_key(|turn| (turn.started_at_ms, turn.finished_at_ms, turn.turn_id.clone()));
    legacy_turns
}

fn parse_legacy_turn_record(entry: &session_tape::TapeEntry) -> Option<TurnLifecycle> {
    let data = entry.event_data()?.clone();
    serde_json::from_value(data).ok()
}

fn parse_iso8601_utc_seconds(input: &str) -> Option<u64> {
    if input.len() != 20 || !input.ends_with('Z') {
        return None;
    }

    let year: i64 = input.get(0..4)?.parse().ok()?;
    let month: i64 = input.get(5..7)?.parse().ok()?;
    let day: i64 = input.get(8..10)?.parse().ok()?;
    let hour: i64 = input.get(11..13)?.parse().ok()?;
    let minute: i64 = input.get(14..16)?.parse().ok()?;
    let second: i64 = input.get(17..19)?.parse().ok()?;

    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 { adjusted_year } else { adjusted_year - 399 } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days_since_epoch = era * 146097 + day_of_era - 719468;
    let total_seconds = days_since_epoch * 86_400 + hour * 3_600 + minute * 60 + second;

    (total_seconds >= 0).then_some((total_seconds as u64) * 1000)
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

    use agent_core::{Message, ModelDisposition, ModelIdentity, Role, ToolCall, ToolResult};
    use agent_runtime::{AgentRuntime, TurnLifecycle};
    use builtin_tools::build_tool_registry;
    use provider_registry::{ModelConfig, ProviderKind, ProviderProfile, ProviderRegistry};
    use session_tape::{SessionProviderBinding, SessionTape, TapeEntry};
    use tokio::sync::broadcast;

    use crate::{model::BootstrapModel, state::AppState};

    use super::{prepare_runtime_sync, rebuild_turn_history_from_tape, sync_runtime_to_registry};

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

        let turns = rebuild_turn_history_from_tape(&tape);

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

        let turns = rebuild_turn_history_from_tape(&tape);

        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0], legacy_turn);
    }
}

pub async fn get_history(State(state): State<SharedState>) -> impl IntoResponse {
    let s = state.lock().expect("lock poisoned");
    let turns = rebuild_turn_history_from_tape(s.runtime.tape());
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
