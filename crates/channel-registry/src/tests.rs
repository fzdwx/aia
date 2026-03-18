use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ChannelProfile, ChannelRegistry};

fn temp_path() -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-channels-{suffix}.json"))
}

#[test]
fn save_and_load_channel_registry_round_trip() {
    let path = temp_path();
    let mut registry = ChannelRegistry::default();
    registry.upsert(ChannelProfile::new_feishu("default", "默认飞书", "cli_app", "secret"));

    registry.save(&path).expect("registry should save");
    let loaded = ChannelRegistry::load_or_default(&path).expect("registry should load");

    assert_eq!(loaded, registry);
    let _ = std::fs::remove_file(path);
}

#[test]
fn removing_unknown_channel_returns_error() {
    let mut registry = ChannelRegistry::default();

    let error = registry.remove("missing").expect_err("missing channel should error");

    assert!(error.to_string().contains("channel 不存在"));
}
