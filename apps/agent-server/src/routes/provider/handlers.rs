use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use provider_registry::ProviderRegistry;
use provider_registry::{ModelConfig, ProviderKind};
use std::sync::{Arc, RwLock};

use crate::{
    routes::common::{
        JsonResponse, error_response, json_response, ok_response, runtime_worker_error_response,
    },
    session_manager::{
        CreateProviderInput, ProviderInfoSnapshot, SessionManagerHandle, SwitchProviderInput,
        UpdateProviderInput, read_lock,
    },
    state::SharedState,
};

use super::dto::{
    CreateProviderRequest, ModelConfigDto, ProviderInfo, ProviderListItem, SwitchProviderRequest,
    UpdateProviderRequest,
};

#[derive(Clone)]
pub(crate) struct ProviderRouteService {
    session_manager: SessionManagerHandle,
    provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
}

impl ProviderRouteService {
    pub(crate) fn new(
        session_manager: SessionManagerHandle,
        provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
        provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    ) -> Self {
        Self { session_manager, provider_registry_snapshot, provider_info_snapshot }
    }

    pub(crate) async fn get_providers(State(state): State<SharedState>) -> Json<ProviderInfo> {
        Json(state.provider_routes.provider_info())
    }

    pub(crate) async fn list_providers(
        State(state): State<SharedState>,
    ) -> Json<Vec<ProviderListItem>> {
        Json(state.provider_routes.provider_list())
    }

    pub(crate) async fn create_provider(
        State(state): State<SharedState>,
        Json(body): Json<CreateProviderRequest>,
    ) -> impl IntoResponse {
        state.provider_routes.create(body).await
    }

    pub(crate) async fn update_provider(
        State(state): State<SharedState>,
        Path(name): Path<String>,
        Json(body): Json<UpdateProviderRequest>,
    ) -> impl IntoResponse {
        state.provider_routes.update(name, body).await
    }

    pub(crate) async fn delete_provider(
        State(state): State<SharedState>,
        Path(name): Path<String>,
    ) -> impl IntoResponse {
        state.provider_routes.delete(name).await
    }

    pub(crate) async fn switch_provider(
        State(state): State<SharedState>,
        Json(body): Json<SwitchProviderRequest>,
    ) -> impl IntoResponse {
        state.provider_routes.switch(body).await
    }

    fn provider_info(&self) -> ProviderInfo {
        let snapshot = read_lock(&self.provider_info_snapshot);
        provider_info_from_snapshot(&snapshot)
    }

    fn provider_list(&self) -> Vec<ProviderListItem> {
        let registry = read_lock(&self.provider_registry_snapshot);
        let active_name = registry.active_provider().map(|provider| provider.name.as_str());
        registry
            .providers()
            .iter()
            .map(|provider| provider_list_item(provider, active_name))
            .collect()
    }

    async fn create(&self, body: CreateProviderRequest) -> JsonResponse {
        let CreateProviderRequest { name, kind, models, active_model, api_key, base_url } = body;
        let kind = match parse_provider_kind(&kind) {
            Ok(kind) => kind,
            Err(response) => return response,
        };

        match self
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

    async fn update(&self, name: String, body: UpdateProviderRequest) -> JsonResponse {
        let UpdateProviderRequest { kind, models, active_model, api_key, base_url } = body;
        let kind = match kind.as_deref().map(parse_provider_kind).transpose() {
            Ok(kind) => kind,
            Err(response) => return response,
        };

        match self
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

    async fn delete(&self, name: String) -> JsonResponse {
        match self.session_manager.delete_provider(name).await {
            Ok(()) => ok_response(),
            Err(error) => runtime_worker_error_response(error),
        }
    }

    async fn switch(&self, body: SwitchProviderRequest) -> JsonResponse {
        match self
            .session_manager
            .switch_provider(SwitchProviderInput { name: body.name, model_id: body.model_id })
            .await
        {
            Ok(info) => {
                json_response(axum::http::StatusCode::OK, provider_info_from_snapshot(&info))
            }
            Err(error) => runtime_worker_error_response(error),
        }
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

pub(super) fn provider_info_from_snapshot(snapshot: &ProviderInfoSnapshot) -> ProviderInfo {
    ProviderInfo {
        name: snapshot.name.clone(),
        model: snapshot.model.clone(),
        connected: snapshot.connected,
    }
}

pub(super) fn provider_list_item(
    provider: &provider_registry::ProviderProfile,
    active_name: Option<&str>,
) -> ProviderListItem {
    ProviderListItem {
        name: provider.name.clone(),
        kind: provider.kind.protocol_name().to_string(),
        models: provider.models.iter().map(ModelConfigDto::from).collect(),
        active_model: provider.active_model.clone(),
        base_url: provider.base_url.clone(),
        active: active_name == Some(provider.name.as_str()),
    }
}
