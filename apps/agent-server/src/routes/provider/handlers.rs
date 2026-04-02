use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use provider_registry::{ModelConfig, ProviderKind};

use crate::{
    session_manager::{CreateProviderInput, UpdateProviderInput, read_lock},
    state::SharedState,
};

use super::{CreateProviderRequest, ModelConfigDto, ProviderListItem, UpdateProviderRequest};
use crate::routes::common::{
    JsonResponse, error_response, ok_response, runtime_worker_error_response,
};

pub(crate) async fn list_providers(
    State(state): State<SharedState>,
) -> Json<Vec<ProviderListItem>> {
    let registry = read_lock(&state.provider_registry_snapshot);
    Json(registry.providers().iter().map(|provider| provider_list_item(provider)).collect())
}

pub(crate) async fn create_provider(
    State(state): State<SharedState>,
    Json(body): Json<CreateProviderRequest>,
) -> impl IntoResponse {
    let CreateProviderRequest { name, kind, models, api_key, base_url } = body;
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
    let UpdateProviderRequest { kind, models, api_key, base_url } = body;
    let kind = match kind.as_deref().map(parse_provider_kind).transpose() {
        Ok(kind) => kind,
        Err(response) => return response,
    };

    match state
        .session_manager
        .update_provider(
            name,
            UpdateProviderInput { kind, models: models.map(models_from_dtos), api_key, base_url },
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

pub(super) fn parse_provider_kind(protocol_name: &str) -> Result<ProviderKind, JsonResponse> {
    match protocol_name {
        "openai-responses" => Ok(ProviderKind::OpenAiResponses),
        "openai-chat-completions" => Ok(ProviderKind::OpenAiChatCompletions),
        _ => Err(error_response(StatusCode::BAD_REQUEST, format!("未知协议：{protocol_name}"))),
    }
}

fn models_from_dtos(dtos: Vec<ModelConfigDto>) -> Vec<ModelConfig> {
    dtos.into_iter().map(ModelConfig::from).collect()
}

pub(super) fn provider_list_item(
    provider: &provider_registry::ProviderProfile,
) -> ProviderListItem {
    ProviderListItem {
        name: provider.name.clone(),
        kind: provider.kind.protocol_name().to_string(),
        models: provider.models.iter().map(ModelConfigDto::from).collect(),
        base_url: provider.base_url.clone(),
    }
}
