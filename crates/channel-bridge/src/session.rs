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
#[path = "../tests/session/mod.rs"]
mod tests;
