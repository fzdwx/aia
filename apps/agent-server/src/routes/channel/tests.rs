use channel_bridge::{ChannelTransport, SupportedChannelDefinition};
use serde_json::json;

use super::{
    config::merge_channel_config,
    dto::{CreateChannelRequest, UpdateChannelRequest},
};

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

#[test]
fn create_channel_request_deserializes_feishu_payload() {
    let parsed: CreateChannelRequest = serde_json::from_value(serde_json::json!({
        "id": "default",
        "name": "默认飞书",
        "transport": "feishu",
        "enabled": true,
        "config": {
            "app_id": "cli_xxx",
            "app_secret": "secret",
            "base_url": "https://open.feishu.cn",
            "require_mention": true,
            "thread_mode": true
        }
    }))
    .expect("create channel request should deserialize");

    assert_eq!(parsed.id, "default");
    assert_eq!(parsed.transport, ChannelTransport::Feishu);
    assert_eq!(parsed.config["thread_mode"], true);
}

#[test]
fn update_channel_request_allows_partial_secret_update() {
    let parsed: UpdateChannelRequest = serde_json::from_value(serde_json::json!({
        "enabled": false,
        "config": {
            "app_secret": ""
        }
    }))
    .expect("update channel request should deserialize");

    assert_eq!(parsed.enabled, Some(false));
    assert_eq!(
        parsed.config.as_ref().and_then(|value| value.get("app_secret")),
        Some(&serde_json::json!(""))
    );
}
