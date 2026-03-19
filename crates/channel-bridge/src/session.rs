use std::sync::Arc;

use agent_prompts::AUTO_COMPRESSION_THRESHOLD;
use agent_store::{
    AiaStore, ChannelMessageReceipt, ChannelSessionBinding, ExternalConversationKey,
};
use async_trait::async_trait;

use crate::ChannelBridgeError;

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSessionInfo {
    pub pressure_ratio: Option<f64>,
}

#[async_trait]
pub trait ChannelSessionService: Send + Sync {
    async fn session_exists(&self, session_id: &str) -> Result<bool, ChannelBridgeError>;

    async fn create_session(&self, title: String) -> Result<String, ChannelBridgeError>;

    async fn session_info(
        &self,
        session_id: &str,
    ) -> Result<ChannelSessionInfo, ChannelBridgeError>;

    async fn auto_compress_session(&self, session_id: &str) -> Result<bool, ChannelBridgeError>;
}

#[async_trait]
pub trait ChannelBindingStore: Send + Sync {
    async fn get_channel_binding(
        &self,
        key: ExternalConversationKey,
    ) -> Result<Option<ChannelSessionBinding>, ChannelBridgeError>;

    async fn upsert_channel_binding(
        &self,
        binding: ChannelSessionBinding,
    ) -> Result<(), ChannelBridgeError>;

    async fn record_channel_message_receipt(
        &self,
        receipt: ChannelMessageReceipt,
    ) -> Result<bool, ChannelBridgeError>;
}

#[async_trait]
impl ChannelBindingStore for Arc<AiaStore> {
    async fn get_channel_binding(
        &self,
        key: ExternalConversationKey,
    ) -> Result<Option<ChannelSessionBinding>, ChannelBridgeError> {
        AiaStore::get_channel_binding_async(self, key)
            .await
            .map_err(|error| ChannelBridgeError::new(error.to_string()))
    }

    async fn upsert_channel_binding(
        &self,
        binding: ChannelSessionBinding,
    ) -> Result<(), ChannelBridgeError> {
        AiaStore::upsert_channel_binding_async(self, binding)
            .await
            .map_err(|error| ChannelBridgeError::new(error.to_string()))
    }

    async fn record_channel_message_receipt(
        &self,
        receipt: ChannelMessageReceipt,
    ) -> Result<bool, ChannelBridgeError> {
        AiaStore::record_channel_message_receipt_async(self, receipt)
            .await
            .map_err(|error| ChannelBridgeError::new(error.to_string()))
    }
}

pub async fn prepare_session_for_turn<S>(
    sessions: &S,
    session_id: &str,
) -> Result<(), ChannelBridgeError>
where
    S: ChannelSessionService + ?Sized,
{
    let info = sessions.session_info(session_id).await?;
    if info.pressure_ratio.is_some_and(|ratio| ratio >= AUTO_COMPRESSION_THRESHOLD) {
        sessions.auto_compress_session(session_id).await?;
    }
    Ok(())
}

pub async fn resolve_or_create_session<S, T>(
    store: &S,
    sessions: &T,
    key: ExternalConversationKey,
    title: String,
) -> Result<String, ChannelBridgeError>
where
    S: ChannelBindingStore + ?Sized,
    T: ChannelSessionService + ?Sized,
{
    if let Some(binding) = store.get_channel_binding(key.clone()).await?
        && sessions.session_exists(&binding.session_id).await?
    {
        return Ok(binding.session_id);
    }

    let session_id = sessions.create_session(title).await?;
    store.upsert_channel_binding(ChannelSessionBinding::new(key, session_id.clone())).await?;
    Ok(session_id)
}

pub async fn record_channel_message_receipt<S>(
    store: &S,
    channel_kind: &str,
    profile_id: &str,
    external_message_id: &str,
    session_id: &str,
) -> Result<bool, ChannelBridgeError>
where
    S: ChannelBindingStore + ?Sized,
{
    store
        .record_channel_message_receipt(ChannelMessageReceipt::new(
            channel_kind,
            profile_id,
            external_message_id,
            session_id,
        ))
        .await
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeSessionService {
        existing: Mutex<HashSet<String>>,
        created_titles: Mutex<Vec<String>>,
        pressure_ratio: Mutex<Option<f64>>,
        compress_calls: Mutex<Vec<String>>,
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
            let session_id = self
                .next_session_id
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
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

        async fn auto_compress_session(
            &self,
            session_id: &str,
        ) -> Result<bool, ChannelBridgeError> {
            self.compress_calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(session_id.to_string());
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
        *service.pressure_ratio.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(0.95);

        prepare_session_for_turn(&service, "session-1").await.expect("session prep should succeed");

        assert_eq!(
            service
                .compress_calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_slice(),
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
            service
                .created_titles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty()
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
}
