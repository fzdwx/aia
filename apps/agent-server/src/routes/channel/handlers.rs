use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use channel_bridge::{ChannelProfile, SupportedChannelDefinition};

use crate::{channel_host::supported_channel_definitions, state::SharedState};

use super::{
    ChannelListItem, CreateChannelRequest, UpdateChannelRequest,
    config::{channel_list_item, merge_channel_config},
    mutation::{ChannelMutation, apply_channel_mutation},
};
use crate::routes::common::{error_response, ok_response};

pub(crate) async fn list_supported_channels(
    State(state): State<SharedState>,
) -> Json<Vec<SupportedChannelDefinition>> {
    Json(supported_channel_definitions(state.channel_adapter_catalog.as_ref()))
}

pub(crate) async fn list_channels(State(state): State<SharedState>) -> Json<Vec<ChannelListItem>> {
    let definitions = supported_channel_definitions(state.channel_adapter_catalog.as_ref());
    let registry = crate::session_manager::read_lock(&state.channel_profile_registry_snapshot);
    Json(
        registry
            .channels()
            .iter()
            .filter_map(|profile| {
                definitions
                    .iter()
                    .find(|definition| definition.transport == profile.transport)
                    .map(|definition| channel_list_item(profile, definition))
            })
            .collect(),
    )
}

pub(crate) async fn create_channel(
    State(state): State<SharedState>,
    Json(body): Json<CreateChannelRequest>,
) -> impl IntoResponse {
    let _mutation_guard = state.channel_mutation_lock.lock().await;
    let transport = body.transport;
    let Some(adapter) = state.channel_adapter_catalog.adapter_for(&transport) else {
        return error_response(
            axum::http::StatusCode::BAD_REQUEST,
            format!("当前服务未注册 transport={} 的 channel adapter", transport),
        );
    };
    if let Err(error) = adapter.validate_config(&body.config) {
        return error_response(axum::http::StatusCode::BAD_REQUEST, error.to_string());
    }

    let profile = ChannelProfile {
        id: body.id,
        name: body.name,
        transport,
        enabled: body.enabled,
        config: body.config,
    };

    let previous_registry =
        crate::session_manager::read_lock(&state.channel_profile_registry_snapshot).clone();
    let mut next_registry = previous_registry.clone();
    let previous_profile = next_registry.get(&profile.id).cloned();
    next_registry.upsert(profile.clone());

    if let Err(response) = apply_channel_mutation(
        &state,
        previous_registry,
        next_registry,
        ChannelMutation::Upsert { next: profile, previous: previous_profile },
    )
    .await
    {
        return response;
    }
    ok_response()
}

pub(crate) async fn update_channel(
    State(state): State<SharedState>,
    Path(channel_id): Path<String>,
    Json(body): Json<UpdateChannelRequest>,
) -> impl IntoResponse {
    let _mutation_guard = state.channel_mutation_lock.lock().await;
    let definitions = supported_channel_definitions(state.channel_adapter_catalog.as_ref());
    let previous_registry =
        crate::session_manager::read_lock(&state.channel_profile_registry_snapshot).clone();
    let updated_profile = {
        let Some(existing) = previous_registry.get(&channel_id).cloned() else {
            return error_response(
                axum::http::StatusCode::NOT_FOUND,
                format!("channel 不存在：{channel_id}"),
            );
        };
        let Some(definition) = definitions.iter().find(|item| item.transport == existing.transport)
        else {
            return error_response(
                axum::http::StatusCode::BAD_REQUEST,
                format!("当前服务未注册 transport={} 的 channel adapter", existing.transport),
            );
        };
        let Some(adapter) = state.channel_adapter_catalog.adapter_for(&existing.transport) else {
            return error_response(
                axum::http::StatusCode::BAD_REQUEST,
                format!("当前服务未注册 transport={} 的 channel adapter", existing.transport),
            );
        };
        let merged_config = match merge_channel_config(&existing.config, body.config, definition) {
            Ok(config) => config,
            Err(error) => {
                return error_response(axum::http::StatusCode::BAD_REQUEST, error);
            }
        };
        if let Err(error) = adapter.validate_config(&merged_config) {
            return error_response(axum::http::StatusCode::BAD_REQUEST, error.to_string());
        }
        ChannelProfile {
            id: existing.id,
            name: body.name.unwrap_or(existing.name),
            transport: existing.transport,
            enabled: body.enabled.unwrap_or(existing.enabled),
            config: merged_config,
        }
    };

    let previous_profile = previous_registry.get(&channel_id).cloned();
    let mut next_registry = previous_registry.clone();
    next_registry.upsert(updated_profile.clone());

    if let Err(response) = apply_channel_mutation(
        &state,
        previous_registry,
        next_registry,
        ChannelMutation::Upsert { next: updated_profile, previous: previous_profile },
    )
    .await
    {
        return response;
    }
    ok_response()
}

pub(crate) async fn delete_channel(
    State(state): State<SharedState>,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    let _mutation_guard = state.channel_mutation_lock.lock().await;
    let previous_registry =
        crate::session_manager::read_lock(&state.channel_profile_registry_snapshot).clone();
    let Some(deleted_profile) = previous_registry.get(&channel_id).cloned() else {
        return error_response(
            axum::http::StatusCode::NOT_FOUND,
            format!("channel 不存在：{channel_id}"),
        );
    };
    let mut next_registry = previous_registry.clone();
    if let Err(error) = next_registry.remove(&channel_id) {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }

    if let Err(response) = apply_channel_mutation(
        &state,
        previous_registry,
        next_registry,
        ChannelMutation::Delete { deleted: deleted_profile },
    )
    .await
    {
        return response;
    };
    ok_response()
}
