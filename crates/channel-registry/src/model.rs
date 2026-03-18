use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelTransport {
    Feishu,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeishuChannelConfig {
    pub app_id: String,
    pub app_secret: String,
    pub base_url: String,
    #[serde(default)]
    pub require_mention: bool,
    #[serde(default)]
    pub thread_mode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelProfile {
    pub id: String,
    pub name: String,
    pub transport: ChannelTransport,
    pub enabled: bool,
    pub config: FeishuChannelConfig,
}

impl ChannelProfile {
    pub fn new_feishu(
        id: impl Into<String>,
        name: impl Into<String>,
        app_id: impl Into<String>,
        app_secret: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            transport: ChannelTransport::Feishu,
            enabled: true,
            config: FeishuChannelConfig {
                app_id: app_id.into(),
                app_secret: app_secret.into(),
                base_url: "https://open.feishu.cn".into(),
                require_mention: true,
                thread_mode: true,
            },
        }
    }
}
