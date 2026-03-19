use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use channel_bridge::{ChannelBridgeError, ChannelProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeishuMessageTarget {
    pub receive_id: String,
    pub receive_id_type: String,
    pub reply_to_message_id: Option<String>,
    pub reply_in_thread: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct FeishuChannelConfig {
    #[tool_schema(description = "App ID", meta(key = "x-label", value = "App ID"))]
    pub(super) app_id: String,
    #[tool_schema(
        description = "编辑时留空表示保持现有 secret",
        meta(key = "x-label", value = "App Secret"),
        meta(key = "x-secret", value = true)
    )]
    pub(super) app_secret: String,
    #[tool_schema(
        description = "Base URL",
        meta(key = "x-label", value = "Base URL"),
        meta(key = "format", value = "uri"),
        meta(key = "default", value = "https://open.feishu.cn")
    )]
    pub(super) base_url: String,
    #[serde(default)]
    #[tool_schema(
        description = "Require mention",
        meta(key = "x-label", value = "Require mention"),
        meta(key = "default", value = true)
    )]
    pub(super) require_mention: bool,
    #[serde(default)]
    #[tool_schema(
        description = "Thread mode",
        meta(key = "x-label", value = "Thread mode"),
        meta(key = "default", value = true)
    )]
    pub(super) thread_mode: bool,
}

pub(super) fn parse_feishu_config(
    config: &Value,
) -> Result<FeishuChannelConfig, ChannelBridgeError> {
    serde_json::from_value(config.clone())
        .map_err(|error| ChannelBridgeError::new(format!("invalid feishu config: {error}")))
}

pub(super) fn feishu_config(profile: &ChannelProfile) -> FeishuChannelConfig {
    parse_feishu_config(&profile.config)
        .unwrap_or_else(|error| unreachable!("feishu adapter received invalid config: {error}"))
}
