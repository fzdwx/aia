use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use provider_registry::{AdapterKind, ModelConfig};

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
    let CreateProviderRequest { id, label, adapter, models, api_key, base_url } = body;
    let adapter = match parse_adapter_kind(&adapter) {
        Ok(adapter) => adapter,
        Err(response) => return response,
    };

    match state
        .session_manager
        .create_provider(CreateProviderInput {
            id,
            label,
            adapter,
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
    Path(id): Path<String>,
    Json(body): Json<UpdateProviderRequest>,
) -> impl IntoResponse {
    let UpdateProviderRequest { label, adapter, models, api_key, base_url } = body;
    let adapter = match adapter.as_deref().map(parse_adapter_kind).transpose() {
        Ok(adapter) => adapter,
        Err(response) => return response,
    };

    match state
        .session_manager
        .update_provider(
            id,
            UpdateProviderInput {
                label,
                adapter,
                models: models.map(models_from_dtos),
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
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.session_manager.delete_provider(id).await {
        Ok(()) => ok_response(),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(super) fn parse_adapter_kind(protocol_name: &str) -> Result<AdapterKind, JsonResponse> {
    match protocol_name {
        "openai-responses" => Ok(AdapterKind::OpenAiResponses),
        "openai-chat-completions" => Ok(AdapterKind::OpenAiChatCompletions),
        _ => Err(error_response(StatusCode::BAD_REQUEST, format!("未知协议：{protocol_name}"))),
    }
}

fn models_from_dtos(dtos: Vec<ModelConfigDto>) -> Vec<ModelConfig> {
    dtos.into_iter().map(ModelConfig::from).collect()
}

pub(super) fn provider_list_item(
    provider: &provider_registry::ProviderAccount,
) -> ProviderListItem {
    ProviderListItem {
        id: provider.id.clone(),
        label: provider.label.clone(),
        adapter: provider.adapter.protocol_name().to_string(),
        models: provider.models.iter().map(ModelConfigDto::from).collect(),
        base_url: provider.endpoint.base_url.clone(),
    }
}
