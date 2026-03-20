use std::sync::Arc;

use agent_store::AiaStore;
use serde_json::json;

use super::*;
use crate::ChannelTransport;

#[tokio::test(flavor = "current_thread")]
async fn profile_registry_round_trip_works_via_store() {
    let store = Arc::new(AiaStore::in_memory().expect("store init"));
    let profile = ChannelProfile::new(
        "default",
        "默认飞书",
        ChannelTransport::Feishu,
        json!({
            "app_id": "cli_app",
            "app_secret": "secret",
        }),
    );

    ChannelProfileRegistry::upsert_into_store(&store, profile.clone())
        .await
        .expect("profile should persist");
    let loaded =
        ChannelProfileRegistry::load_from_store(&store).await.expect("profiles should load");

    assert_eq!(loaded.channels(), &[profile]);
}
