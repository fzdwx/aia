use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use channel_registry::{ChannelProfile, ChannelTransport, FeishuChannelConfig};
use serde::{Deserialize, Serialize};

use crate::{channel_runtime::sync_channel_runtime, state::SharedState};

use super::common::{JsonResponse, error_response, ok_response};

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChannelListItem {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub enabled: bool,
    pub app_id: String,
    pub app_secret_set: bool,
    pub base_url: String,
    pub require_mention: bool,
    pub thread_mode: bool,
}

#[derive(Deserialize)]
pub(crate) struct CreateChannelRequest {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub base_url: String,
    pub require_mention: bool,
    pub thread_mode: bool,
}

#[derive(Deserialize)]
pub(crate) struct UpdateChannelRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub base_url: Option<String>,
    pub require_mention: Option<bool>,
    pub thread_mode: Option<bool>,
}

fn channel_list_item(profile: &ChannelProfile) -> ChannelListItem {
    ChannelListItem {
        id: profile.id.clone(),
        name: profile.name.clone(),
        transport: match profile.transport {
            ChannelTransport::Feishu => "feishu".into(),
        },
        enabled: profile.enabled,
        app_id: profile.config.app_id.clone(),
        app_secret_set: !profile.config.app_secret.is_empty(),
        base_url: profile.config.base_url.clone(),
        require_mention: profile.config.require_mention,
        thread_mode: profile.config.thread_mode,
    }
}

fn parse_transport(value: &str) -> Result<ChannelTransport, JsonResponse> {
    match value {
        "feishu" => Ok(ChannelTransport::Feishu),
        _ => Err(error_response(
            axum::http::StatusCode::BAD_REQUEST,
            format!("未知通道协议：{value}"),
        )),
    }
}

pub(crate) async fn list_channels(State(state): State<SharedState>) -> Json<Vec<ChannelListItem>> {
    let registry = crate::session_manager::read_lock(&state.channel_registry_snapshot);
    Json(registry.channels().iter().map(channel_list_item).collect())
}

pub(crate) async fn create_channel(
    State(state): State<SharedState>,
    Json(body): Json<CreateChannelRequest>,
) -> impl IntoResponse {
    let transport = match parse_transport(&body.transport) {
        Ok(transport) => transport,
        Err(response) => return response,
    };
    let profile = ChannelProfile {
        id: body.id,
        name: body.name,
        transport,
        enabled: body.enabled,
        config: FeishuChannelConfig {
            app_id: body.app_id,
            app_secret: body.app_secret,
            base_url: body.base_url,
            require_mention: body.require_mention,
            thread_mode: body.thread_mode,
        },
    };

    let save_result = {
        let mut registry = crate::session_manager::write_lock(&state.channel_registry_snapshot);
        registry.upsert(profile);
        registry.save(&state.channel_registry_path)
    };

    if let Err(error) = save_result {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    if let Err(error) = sync_channel_runtime(state.as_ref()).await {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    ok_response()
}

pub(crate) async fn update_channel(
    State(state): State<SharedState>,
    Path(channel_id): Path<String>,
    Json(body): Json<UpdateChannelRequest>,
) -> impl IntoResponse {
    let save_result = {
        let mut registry = crate::session_manager::write_lock(&state.channel_registry_snapshot);
        let Some(existing) = registry.get(&channel_id).cloned() else {
            return error_response(
                axum::http::StatusCode::NOT_FOUND,
                format!("channel 不存在：{channel_id}"),
            );
        };
        let profile = ChannelProfile {
            id: existing.id,
            name: body.name.unwrap_or(existing.name),
            transport: existing.transport,
            enabled: body.enabled.unwrap_or(existing.enabled),
            config: FeishuChannelConfig {
                app_id: body.app_id.unwrap_or(existing.config.app_id),
                app_secret: body
                    .app_secret
                    .filter(|secret| !secret.trim().is_empty())
                    .unwrap_or(existing.config.app_secret),
                base_url: body.base_url.unwrap_or(existing.config.base_url),
                require_mention: body.require_mention.unwrap_or(existing.config.require_mention),
                thread_mode: body.thread_mode.unwrap_or(existing.config.thread_mode),
            },
        };
        registry.upsert(profile);
        registry.save(&state.channel_registry_path)
    };

    if let Err(error) = save_result {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    if let Err(error) = sync_channel_runtime(state.as_ref()).await {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    ok_response()
}

pub(crate) async fn delete_channel(
    State(state): State<SharedState>,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    let save_result = {
        let mut registry = crate::session_manager::write_lock(&state.channel_registry_snapshot);
        match registry.remove(&channel_id) {
            Ok(()) => registry.save(&state.channel_registry_path),
            Err(error) => {
                return error_response(axum::http::StatusCode::NOT_FOUND, error.to_string());
            }
        }
    };

    if let Err(error) = save_result {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    if let Err(error) = sync_channel_runtime(state.as_ref()).await {
        return error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    ok_response()
}

#[cfg(test)]
mod tests {
    use super::parse_transport;

    #[test]
    fn parse_transport_accepts_feishu() {
        assert!(parse_transport("feishu").is_ok());
    }

    #[test]
    fn parse_transport_rejects_unknown_transport() {
        assert!(parse_transport("slack").is_err());
    }
}
