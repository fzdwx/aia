use std::{collections::HashSet, sync::Mutex};

use super::*;

#[derive(Default)]
struct FakeSessionService {
    existing: Mutex<HashSet<String>>,
    created_titles: Mutex<Vec<String>>,
    pressure_ratio: Mutex<Option<f64>>,
    compress_calls: Mutex<Vec<String>>,
    compress_error: Mutex<Option<String>>,
    next_session_id: Mutex<String>,
}

#[async_trait]
impl ChannelSessionService for FakeSessionService {
    async fn session_exists(&self, session_id: &str) -> Result<bool, ChannelBridgeError> {
        Ok(self
            .existing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(session_id))
    }

    async fn create_session(&self, title: String) -> Result<String, ChannelBridgeError> {
        self.created_titles.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(title);
        let session_id =
            self.next_session_id.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        self.existing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(session_id.clone());
        Ok(session_id)
    }

    async fn session_info(
        &self,
        _session_id: &str,
    ) -> Result<ChannelSessionInfo, ChannelBridgeError> {
        Ok(ChannelSessionInfo {
            pressure_ratio: *self
                .pressure_ratio
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        })
    }

    async fn auto_compress_session(&self, session_id: &str) -> Result<bool, ChannelBridgeError> {
        self.compress_calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(session_id.to_string());
        if let Some(message) =
            self.compress_error.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
        {
            return Err(ChannelBridgeError::new(message));
        }
        Ok(true)
    }
}

fn sample_key() -> ExternalConversationKey {
    ExternalConversationKey {
        channel_kind: "feishu".into(),
        profile_id: "default".into(),
        scope: "p2p".into(),
        conversation_key: "open_id:ou_123".into(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn prepare_session_for_turn_triggers_auto_compress_when_over_threshold() {
    let service = FakeSessionService::default();
    *service.pressure_ratio.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(0.95);

    prepare_session_for_turn(&service, "session-1").await.expect("session prep should succeed");

    assert_eq!(
        service.compress_calls.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_slice(),
        ["session-1"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn prepare_session_for_turn_does_not_fail_when_auto_compress_errors() {
    let service = FakeSessionService::default();
    *service.pressure_ratio.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(0.95);
    *service.compress_error.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
        Some("upstream 502".into());

    prepare_session_for_turn(&service, "session-1")
        .await
        .expect("session prep should ignore auto-compress failure");

    assert_eq!(
        service.compress_calls.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_slice(),
        ["session-1"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_or_create_session_reuses_existing_live_binding() {
    let store = Arc::new(AiaStore::in_memory().expect("memory store"));
    let key = sample_key();
    store
        .upsert_channel_binding_async(ChannelSessionBinding::new(key.clone(), "session-live"))
        .await
        .expect("binding should save");

    let service = FakeSessionService::default();
    service
        .existing
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert("session-live".into());

    let session_id = resolve_or_create_session(&store, &service, key, "ignored".into())
        .await
        .expect("live binding should resolve");

    assert_eq!(session_id, "session-live");
    assert!(
        service.created_titles.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_empty()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_or_create_session_recreates_stale_binding() {
    let store = Arc::new(AiaStore::in_memory().expect("memory store"));
    let key = sample_key();
    store
        .upsert_channel_binding_async(ChannelSessionBinding::new(key.clone(), "session-stale"))
        .await
        .expect("binding should save");

    let service = FakeSessionService::default();
    *service.next_session_id.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
        "session-new".into();

    let session_id =
        resolve_or_create_session(&store, &service, key.clone(), "Feishu DM · demo".into())
            .await
            .expect("stale binding should be rebound");

    assert_eq!(session_id, "session-new");
    let rebound = store
        .get_channel_binding_async(key)
        .await
        .expect("binding should reload")
        .expect("binding should exist");
    assert_eq!(rebound.session_id, "session-new");
}
