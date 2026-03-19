use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use channel_bridge::{
    ChannelProfile, ChannelProfileRegistry, ChannelTransport, SupportedChannelDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    channel_host::{supported_channel_definitions, sync_channel_runtime},
    state::SharedState,
};

use super::common::{JsonResponse, error_response, ok_response};

enum ChannelMutation {
    Upsert { next: ChannelProfile, previous: Option<ChannelProfile> },
    Delete { deleted: ChannelProfile },
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct ChannelListItem {
    pub id: String,
    pub name: String,
    pub transport: ChannelTransport,
    pub enabled: bool,
    pub config: Value,
    pub secret_fields_set: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct CreateChannelRequest {
    pub id: String,
    pub name: String,
    pub transport: ChannelTransport,
    pub enabled: bool,
    pub config: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpdateChannelRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<Value>,
}

fn sanitize_config_for_display(
    config: &Value,
    definition: &SupportedChannelDefinition,
) -> (Value, Vec<String>) {
    let mut sanitized = config.clone();
    let Some(object) = sanitized.as_object_mut() else {
        return (sanitized, Vec::new());
    };

    let mut secret_fields_set = Vec::new();
    for key in secret_field_keys(definition) {
        let is_set =
            object.get(&key).and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty());
        if is_set {
            secret_fields_set.push(key.clone());
        }
        object.insert(key, Value::String(String::new()));
    }

    (sanitized, secret_fields_set)
}

fn merge_channel_config(
    existing: &Value,
    patch: Option<Value>,
    definition: &SupportedChannelDefinition,
) -> Result<Value, String> {
    let mut merged = existing.clone();
    let Some(patch) = patch else {
        return Ok(merged);
    };

    let Some(merged_object) = merged.as_object_mut() else {
        return Err("channel config 必须是对象".into());
    };
    let Some(patch_object) = patch.as_object() else {
        return Err("channel config patch 必须是对象".into());
    };
    let secret_keys = secret_field_keys(definition);

    for (key, value) in patch_object {
        let secret_field = secret_keys.iter().any(|item| item == key);
        if secret_field && value.as_str().is_some_and(|secret| secret.trim().is_empty()) {
            continue;
        }
        merged_object.insert(key.clone(), value.clone());
    }

    Ok(merged)
}

fn secret_field_keys(definition: &SupportedChannelDefinition) -> Vec<String> {
    definition
        .config_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .filter_map(|(key, schema)| {
                    schema
                        .get("x-secret")
                        .and_then(Value::as_bool)
                        .is_some_and(|secret| secret)
                        .then_some(key.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn channel_list_item(
    profile: &ChannelProfile,
    definition: &SupportedChannelDefinition,
) -> ChannelListItem {
    let (config, secret_fields_set) = sanitize_config_for_display(&profile.config, definition);
    ChannelListItem {
        id: profile.id.clone(),
        name: profile.name.clone(),
        transport: profile.transport.clone(),
        enabled: profile.enabled,
        config,
        secret_fields_set,
    }
}

async fn apply_channel_mutation(
    state: &SharedState,
    previous_registry: ChannelProfileRegistry,
    next_registry: ChannelProfileRegistry,
    mutation: ChannelMutation,
) -> Result<(), JsonResponse> {
    persist_channel_mutation(&state.store, &mutation).await.map_err(|error| {
        error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    })?;

    *crate::session_manager::write_lock(&state.channel_profile_registry_snapshot) = next_registry;
    if let Err(sync_error) = sync_channel_runtime(state.as_ref()).await {
        let rollback_error = rollback_channel_mutation(state, previous_registry, mutation).await;
        let message = match rollback_error {
            Ok(()) => format!("channel runtime 同步失败，已回滚：{sync_error}"),
            Err(rollback_error) => {
                format!(
                    "channel runtime 同步失败且回滚失败：{sync_error}；回滚错误：{rollback_error}"
                )
            }
        };
        return Err(error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, message));
    }

    Ok(())
}

async fn persist_channel_mutation(
    store: &std::sync::Arc<agent_store::AiaStore>,
    mutation: &ChannelMutation,
) -> Result<(), channel_bridge::ChannelBridgeError> {
    match mutation {
        ChannelMutation::Upsert { next, .. } => {
            ChannelProfileRegistry::upsert_into_store(store, next.clone()).await
        }
        ChannelMutation::Delete { deleted } => {
            ChannelProfileRegistry::delete_from_store(store, &deleted.id).await
        }
    }
}

async fn rollback_channel_mutation(
    state: &SharedState,
    previous_registry: ChannelProfileRegistry,
    mutation: ChannelMutation,
) -> Result<(), String> {
    match mutation {
        ChannelMutation::Upsert { previous, next } => match previous {
            Some(previous) => ChannelProfileRegistry::upsert_into_store(&state.store, previous)
                .await
                .map_err(|error| error.to_string())?,
            None => ChannelProfileRegistry::delete_from_store(&state.store, &next.id)
                .await
                .map_err(|error| error.to_string())?,
        },
        ChannelMutation::Delete { deleted } => {
            ChannelProfileRegistry::upsert_into_store(&state.store, deleted)
                .await
                .map_err(|error| error.to_string())?
        }
    }

    *crate::session_manager::write_lock(&state.channel_profile_registry_snapshot) =
        previous_registry;
    sync_channel_runtime(state.as_ref()).await
}

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

#[cfg(test)]
mod tests {
    use channel_bridge::{ChannelTransport, SupportedChannelDefinition};
    use serde_json::json;

    use super::merge_channel_config;

    #[test]
    fn merge_channel_config_keeps_secret_when_patch_is_blank() {
        let definition = SupportedChannelDefinition {
            transport: ChannelTransport::Feishu,
            label: "Feishu".into(),
            description: None,
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "app_secret": {
                        "type": "string",
                        "x-secret": true
                    }
                },
                "required": ["app_secret"],
                "additionalProperties": false
            }),
        };

        let merged = merge_channel_config(
            &json!({ "app_secret": "secret", "base_url": "https://open.feishu.cn" }),
            Some(json!({ "app_secret": "", "base_url": "https://proxy" })),
            &definition,
        )
        .expect("config should merge");

        assert_eq!(merged["app_secret"], "secret");
        assert_eq!(merged["base_url"], "https://proxy");
    }
}
