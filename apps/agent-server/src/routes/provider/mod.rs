use axum::{
    Router,
    routing::{get, post, put},
};
use provider_registry::{ModelConfig, ModelLimit};
use serde::{Deserialize, Serialize};

use crate::state::SharedState;

#[derive(Serialize)]
pub(crate) struct ProviderListItem {
    pub id: String,
    pub label: String,
    pub adapter: String,
    pub models: Vec<ModelConfigDto>,
    pub base_url: String,
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
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CreateProviderRequest {
    pub id: String,
    pub label: String,
    pub adapter: String,
    pub models: Vec<ModelConfigDto>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Deserialize)]
pub(crate) struct UpdateProviderRequest {
    pub label: Option<String>,
    pub adapter: Option<String>,
    pub models: Option<Vec<ModelConfigDto>>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

mod handlers;
#[cfg(test)]
#[path = "../../../tests/routes/provider/mod.rs"]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/providers", post(handlers::create_provider))
        .route("/api/providers/list", get(handlers::list_providers))
        .route(
            "/api/providers/{id}",
            put(handlers::update_provider).delete(handlers::delete_provider),
        )
}
