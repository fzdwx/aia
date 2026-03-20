use axum::{
    Router,
    routing::{get, post, put},
};
use provider_registry::{ModelConfig, ModelLimit};
use serde::{Deserialize, Serialize};

use crate::state::SharedState;

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

mod handlers;
#[cfg(test)]
#[path = "../../../tests/routes/provider/mod.rs"]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/providers", get(handlers::get_providers).post(handlers::create_provider))
        .route("/api/providers/list", get(handlers::list_providers))
        .route(
            "/api/providers/{name}",
            put(handlers::update_provider).delete(handlers::delete_provider),
        )
        .route("/api/providers/switch", post(handlers::switch_provider))
}
