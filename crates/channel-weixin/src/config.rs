use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use channel_bridge::{ChannelBridgeError, ChannelProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use weixin_client::WeixinClientConfig;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WeixinChannelConfig {
    #[tool_schema(
        description = "Bot token used for iLink bot requests",
        meta(key = "x-label", value = "Bot token"),
        meta(key = "x-secret", value = true)
    )]
    pub(super) bot_token: String,
    #[serde(default)]
    #[tool_schema(
        description = "Bound Weixin bot account identifier",
        meta(key = "x-label", value = "Account ID")
    )]
    pub(super) account_id: Option<String>,
    #[serde(default)]
    #[tool_schema(
        description = "Bound Weixin user identifier",
        meta(key = "x-label", value = "User ID")
    )]
    pub(super) user_id: Option<String>,
}

pub(super) fn parse_weixin_config(
    config: &Value,
) -> Result<WeixinChannelConfig, ChannelBridgeError> {
    let mut sanitized = config.clone();
    if let Some(object) = sanitized.as_object_mut() {
        object.remove("base_url");
        object.remove("cdn_base_url");
    }
    let parsed: WeixinChannelConfig = serde_json::from_value(sanitized)
        .map_err(|error| ChannelBridgeError::new(format!("invalid weixin config: {error}")))?;
    if parsed.bot_token.trim().is_empty() {
        return Err(ChannelBridgeError::new("invalid weixin config: bot token must not be empty"));
    }
    Ok(parsed)
}

pub(super) fn weixin_config(profile: &ChannelProfile) -> WeixinChannelConfig {
    parse_weixin_config(&profile.config)
        .unwrap_or_else(|error| unreachable!("weixin adapter received invalid config: {error}"))
}

pub(super) fn weixin_client_config(profile: &ChannelProfile) -> WeixinClientConfig {
    let config = weixin_config(profile);
    WeixinClientConfig::new("", Some(config.bot_token.as_str()))
}
