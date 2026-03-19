use channel_bridge::ChannelTransport;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
