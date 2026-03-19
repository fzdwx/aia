use channel_bridge::{ChannelTransport, SupportedChannelDefinition};
use serde_json::json;

use super::config::merge_channel_config;

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
