use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::{ChannelProfile, ChannelRegistry, ChannelTransport};

fn temp_path() -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-channels-{suffix}.json"))
}

#[test]
fn save_and_load_channel_registry_round_trip() {
    let path = temp_path();
    let mut registry = ChannelRegistry::default();
    registry.upsert(ChannelProfile::new(
        "default",
        "默认飞书",
        ChannelTransport::Feishu,
        json!({
            "app_id": "cli_app",
            "app_secret": "secret",
            "base_url": "https://open.feishu.cn",
            "require_mention": true,
            "thread_mode": true
        }),
    ));

    registry.save(&path).expect("registry should save");
    let loaded = ChannelRegistry::load_or_default(&path).expect("registry should load");

    assert_eq!(loaded, registry);
    let _ = std::fs::remove_file(path);
}

#[test]
fn load_legacy_feishu_config_shape() {
    let path = temp_path();
    std::fs::write(
        &path,
        r#"{
  "channels": [
    {
      "id": "default",
      "name": "默认飞书",
      "transport": "feishu",
      "enabled": true,
      "config": {
        "app_id": "cli_app",
        "app_secret": "secret",
        "base_url": "https://open.feishu.cn",
        "require_mention": true,
        "thread_mode": true
      }
    }
  ]
}"#,
    )
    .expect("legacy channels.json should write");

    let loaded = ChannelRegistry::load_or_default(&path).expect("legacy registry should load");
    let profile = loaded.channels().first().expect("legacy profile should exist");

    assert_eq!(profile.transport, crate::ChannelTransport::Feishu);
    assert_eq!(profile.config["app_id"], "cli_app");
    let _ = std::fs::remove_file(path);
}

#[test]
fn serialize_channel_profile_with_raw_config() {
    let profile = ChannelProfile::new(
        "default",
        "默认飞书",
        ChannelTransport::Feishu,
        json!({
            "app_id": "cli_app",
            "app_secret": "secret",
            "base_url": "https://open.feishu.cn",
            "require_mention": true,
            "thread_mode": true
        }),
    );

    let json = serde_json::to_value(&profile).expect("profile should serialize");

    assert_eq!(json["transport"], "feishu");
    assert_eq!(json["config"]["app_id"], "cli_app");
}

#[test]
fn removing_unknown_channel_returns_error() {
    let mut registry = ChannelRegistry::default();

    let error = registry.remove("missing").expect_err("missing channel should error");

    assert!(error.to_string().contains("channel 不存在"));
}
