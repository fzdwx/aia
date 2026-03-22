use axum::{
    Router,
    routing::{get, post, put},
};
use channel_bridge::ChannelTransport;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::SharedState;

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

#[derive(Deserialize)]
pub(crate) struct WeixinLoginQrRequest {}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct WeixinLoginQrResponse {
    pub qrcode: String,
    pub qrcode_url: String,
}

#[derive(Deserialize)]
pub(crate) struct WeixinLoginStatusRequest {
    pub qrcode: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct WeixinLoginStatusResponse {
    pub status: String,
    pub bot_token: Option<String>,
    pub account_id: Option<String>,
    pub user_id: Option<String>,
}

mod config;
mod handlers;
mod mutation;
#[cfg(test)]
#[path = "../../../tests/routes/channel/mod.rs"]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/channels", get(handlers::list_channels).post(handlers::create_channel))
        .route("/api/channels/catalog", get(handlers::list_supported_channels))
        .route("/api/channels/weixin/login/qr", post(handlers::create_weixin_login_qr))
        .route("/api/channels/weixin/login/status", post(handlers::poll_weixin_login_status))
        .route("/api/channels/{id}", put(handlers::update_channel).delete(handlers::delete_channel))
}
