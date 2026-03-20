use std::sync::Arc;

use super::*;

fn sample_key() -> ExternalConversationKey {
    ExternalConversationKey {
        channel_kind: "feishu".into(),
        profile_id: "default".into(),
        scope: "group".into(),
        conversation_key: "chat:oc_123".into(),
    }
}

#[test]
fn channel_binding_round_trip_works() {
    let store = AiaStore::in_memory().expect("store init");
    let key = sample_key();
    let binding = ChannelSessionBinding::new(key.clone(), "session-1");

    store.upsert_channel_binding(&binding).expect("binding should save");
    let found = store.get_channel_binding(&key).expect("binding should load");

    assert_eq!(found, Some(binding));
}

#[test]
fn duplicate_message_receipt_is_ignored() {
    let store = AiaStore::in_memory().expect("store init");
    let receipt = ChannelMessageReceipt::new("feishu", "default", "om_123", "session-1");

    let first = store.record_channel_message_receipt(&receipt).expect("first receipt should save");
    let second = store
        .record_channel_message_receipt(&receipt)
        .expect("duplicate receipt should be handled");

    assert!(first);
    assert!(!second);
}

#[tokio::test(flavor = "current_thread")]
async fn async_channel_binding_round_trip_works() {
    let store = Arc::new(AiaStore::in_memory().expect("store init"));
    let key = sample_key();
    let binding = ChannelSessionBinding::new(key.clone(), "session-2");

    store.upsert_channel_binding_async(binding.clone()).await.expect("binding should save async");
    let found = store.get_channel_binding_async(key).await.expect("binding should load async");

    assert_eq!(found, Some(binding));
}

#[tokio::test(flavor = "current_thread")]
async fn delete_channel_bindings_by_session_id_removes_binding() {
    let store = Arc::new(AiaStore::in_memory().expect("store init"));
    let key = sample_key();
    let binding = ChannelSessionBinding::new(key.clone(), "session-stale");

    store.upsert_channel_binding_async(binding).await.expect("binding should save async");

    let deleted = store
        .delete_channel_bindings_by_session_id_async("session-stale")
        .await
        .expect("delete bindings should succeed");
    let found = store.get_channel_binding_async(key).await.expect("binding should load async");

    assert_eq!(deleted, 1);
    assert_eq!(found, None);
}

#[tokio::test(flavor = "current_thread")]
async fn channel_profile_round_trip_works() {
    let store = Arc::new(AiaStore::in_memory().expect("store init"));
    let profile =
        StoredChannelProfile::new("default", "默认飞书", "feishu", true, r#"{"app_id":"cli_app"}"#);

    store.upsert_channel_profile_async(profile.clone()).await.expect("profile should save async");

    let listed = store.list_channel_profiles_async().await.expect("profiles should list");

    assert_eq!(listed, vec![profile]);
}
