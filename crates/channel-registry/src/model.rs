use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelTransport {
    Feishu,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelProfile {
    pub id: String,
    pub name: String,
    pub transport: ChannelTransport,
    pub enabled: bool,
    #[serde(default)]
    pub config: Value,
}

impl ChannelProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        transport: ChannelTransport,
        config: Value,
    ) -> Self {
        Self { id: id.into(), name: name.into(), transport, enabled: true, config }
    }
}
