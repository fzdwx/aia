use std::sync::Arc;

use agent_store::{AiaStore, ChannelSessionBinding, ExternalConversationKey};
use async_trait::async_trait;
use channel_bridge::{
    ChannelBindingStore, ChannelBridgeError, ChannelRuntimeEvent, ChannelRuntimeHost,
    ChannelSessionInfo, ChannelSessionService,
};

use crate::{
    session_manager::{RuntimeWorkerError, SessionManagerHandle},
    sse::SsePayload,
};

use super::mapping::map_sse_payload;

#[derive(Clone)]
pub(super) struct AgentServerChannelHost {
    store: Arc<AiaStore>,
    session_manager: SessionManagerHandle,
    broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
}

impl AgentServerChannelHost {
    pub(super) fn new(
        store: Arc<AiaStore>,
        session_manager: SessionManagerHandle,
        broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
    ) -> Self {
        Self { store, session_manager, broadcast_tx }
    }
}

#[async_trait]
impl ChannelBindingStore for AgentServerChannelHost {
    async fn get_channel_binding(
        &self,
        key: ExternalConversationKey,
    ) -> Result<Option<ChannelSessionBinding>, ChannelBridgeError> {
        ChannelBindingStore::get_channel_binding(&self.store, key).await
    }

    async fn upsert_channel_binding(
        &self,
        binding: ChannelSessionBinding,
    ) -> Result<(), ChannelBridgeError> {
        ChannelBindingStore::upsert_channel_binding(&self.store, binding).await
    }

    async fn record_channel_message_receipt(
        &self,
        receipt: agent_store::ChannelMessageReceipt,
    ) -> Result<bool, ChannelBridgeError> {
        ChannelBindingStore::record_channel_message_receipt(&self.store, receipt).await
    }
}

#[async_trait]
impl ChannelSessionService for AgentServerChannelHost {
    async fn session_exists(&self, session_id: &str) -> Result<bool, ChannelBridgeError> {
        Ok(self.session_manager.get_session_info(session_id.to_string()).await.is_ok())
    }

    async fn create_session(&self, title: String) -> Result<String, ChannelBridgeError> {
        self.session_manager
            .create_session(Some(title))
            .await
            .map(|session| session.id)
            .map_err(runtime_error_to_bridge_error)
    }

    async fn session_info(
        &self,
        session_id: &str,
    ) -> Result<ChannelSessionInfo, ChannelBridgeError> {
        let stats = self
            .session_manager
            .get_session_info(session_id.to_string())
            .await
            .map_err(runtime_error_to_bridge_error)?;
        Ok(ChannelSessionInfo { pressure_ratio: stats.pressure_ratio })
    }

    async fn auto_compress_session(&self, session_id: &str) -> Result<bool, ChannelBridgeError> {
        self.session_manager
            .auto_compress_session(session_id.to_string())
            .await
            .map_err(runtime_error_to_bridge_error)
    }
}

#[async_trait]
impl ChannelRuntimeHost for AgentServerChannelHost {
    async fn submit_turn(&self, session_id: String, prompt: String) -> Result<String, String> {
        self.session_manager
            .submit_turn(session_id, vec![prompt])
            .await
            .map_err(runtime_error_to_string)
    }

    fn subscribe_runtime_events(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ChannelRuntimeEvent> {
        let mut rx = self.broadcast_tx.subscribe();
        let (tx, out) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(payload) => {
                        if let Some(event) = map_sse_payload(payload)
                            && tx.send(event).is_err()
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        out
    }
}

fn runtime_error_to_bridge_error(error: RuntimeWorkerError) -> ChannelBridgeError {
    ChannelBridgeError::new(error.message)
}

fn runtime_error_to_string(error: RuntimeWorkerError) -> String {
    error.message
}
