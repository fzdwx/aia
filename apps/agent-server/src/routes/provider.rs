use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use provider_registry::{ModelConfig, ModelLimit, ProviderKind};
use serde::{Deserialize, Serialize};

use crate::{
    session_manager::{
        CreateProviderInput, ProviderInfoSnapshot, SwitchProviderInput, UpdateProviderInput,
    },
    state::SharedState,
};

use super::common::{JsonResponse, error_response, ok_response, runtime_worker_error_response};

#[derive(Debug, Serialize)]
pub(crate) struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

#[derive(Serialize)]
pub(crate) struct ProviderListItem {
    pub name: String,
    pub kind: String,
    pub models: Vec<ModelConfigDto>,
    pub active_model: Option<String>,
    pub base_url: String,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelLimitDto {
    pub context: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ModelConfigDto {
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
    fn from(model: &ModelConfig) -> Self {
        Self {
            id: model.id.clone(),
            display_name: model.display_name.clone(),
            limit: model.limit.as_ref().map(ModelLimitDto::from),
            default_temperature: model.default_temperature,
            supports_reasoning: model.supports_reasoning,
            reasoning_effort: model.reasoning_effort.clone(),
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
pub(crate) struct CreateProviderRequest {
    pub name: String,
    pub kind: String,
    pub models: Vec<ModelConfigDto>,
    pub active_model: Option<String>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Deserialize)]
pub(crate) struct UpdateProviderRequest {
    pub kind: Option<String>,
    pub models: Option<Vec<ModelConfigDto>>,
    pub active_model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SwitchProviderRequest {
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

fn parse_provider_kind(protocol_name: &str) -> Result<ProviderKind, JsonResponse> {
    match protocol_name {
        "openai-responses" => Ok(ProviderKind::OpenAiResponses),
        "openai-chat-completions" => Ok(ProviderKind::OpenAiChatCompletions),
        _ => Err(error_response(StatusCode::BAD_REQUEST, format!("未知协议：{protocol_name}"))),
    }
}

fn models_from_dtos(dtos: Vec<ModelConfigDto>) -> Vec<ModelConfig> {
    dtos.into_iter().map(ModelConfig::from).collect()
}

pub(crate) async fn get_providers(State(state): State<SharedState>) -> Json<ProviderInfo> {
    let snapshot = crate::session_manager::read_lock(&state.provider_info_snapshot);
    Json(provider_info_from_snapshot(&snapshot))
}

pub(crate) async fn list_providers(
    State(state): State<SharedState>,
) -> Json<Vec<ProviderListItem>> {
    let registry = crate::session_manager::read_lock(&state.provider_registry_snapshot);
    let active_name = registry.active_provider().map(|provider| provider.name.clone());
    let items = registry
        .providers()
        .iter()
        .map(|provider| ProviderListItem {
            name: provider.name.clone(),
            kind: provider.kind.protocol_name().to_string(),
            models: provider.models.iter().map(ModelConfigDto::from).collect(),
            active_model: provider.active_model.clone(),
            base_url: provider.base_url.clone(),
            active: active_name.as_deref() == Some(&provider.name),
        })
        .collect();
    Json(items)
}

pub(crate) async fn create_provider(
    State(state): State<SharedState>,
    Json(body): Json<CreateProviderRequest>,
) -> impl IntoResponse {
    let CreateProviderRequest { name, kind, models, active_model, api_key, base_url } = body;
    let kind = match parse_provider_kind(&kind) {
        Ok(kind) => kind,
        Err(response) => return response,
    };

    match state
        .session_manager
        .create_provider(CreateProviderInput {
            name,
            kind,
            models: models_from_dtos(models),
            active_model,
            api_key,
            base_url,
        })
        .await
    {
        Ok(()) => ok_response(),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn update_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateProviderRequest>,
) -> impl IntoResponse {
    let UpdateProviderRequest { kind, models, active_model, api_key, base_url } = body;
    let kind = match kind.as_deref().map(parse_provider_kind).transpose() {
        Ok(kind) => kind,
        Err(response) => return response,
    };

    match state
        .session_manager
        .update_provider(
            name,
            UpdateProviderInput {
                kind,
                models: models.map(models_from_dtos),
                active_model,
                api_key,
                base_url,
            },
        )
        .await
    {
        Ok(()) => ok_response(),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn delete_provider(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.session_manager.delete_provider(name).await {
        Ok(()) => ok_response(),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn switch_provider(
    State(state): State<SharedState>,
    Json(body): Json<SwitchProviderRequest>,
) -> impl IntoResponse {
    match state
        .session_manager
        .switch_provider(SwitchProviderInput { name: body.name, model_id: body.model_id })
        .await
    {
        Ok(info) => (StatusCode::OK, Json(serde_json::json!(provider_info_from_snapshot(&info)))),
        Err(error) => runtime_worker_error_response(error),
    }
}
