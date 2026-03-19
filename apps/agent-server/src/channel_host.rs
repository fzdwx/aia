use std::sync::Arc;

use agent_store::{AiaStore, ChannelSessionBinding, ExternalConversationKey};
use async_trait::async_trait;
use channel_bridge::{
    ChannelAdapterCatalog, ChannelBindingStore, ChannelBridgeError, ChannelCurrentTurnSnapshot,
    ChannelRuntimeEvent, ChannelRuntimeHost, ChannelRuntimeSupervisor, ChannelSessionInfo,
    ChannelSessionService, ChannelTurnStatus, SupportedChannelDefinition,
};
use channel_feishu::build_feishu_runtime_adapter;

use crate::{
    runtime_worker::CurrentTurnSnapshot,
    session_manager::{RuntimeWorkerError, SessionManagerHandle, read_lock},
    sse::{SsePayload, TurnStatus},
    state::AppState,
};

#[derive(Clone)]
struct AgentServerChannelHost {
    store: Arc<AiaStore>,
    session_manager: SessionManagerHandle,
    broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
}

pub fn build_channel_adapter_catalog(
    store: Arc<AiaStore>,
    session_manager: SessionManagerHandle,
    broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
) -> ChannelAdapterCatalog {
    let host = Arc::new(AgentServerChannelHost { store, session_manager, broadcast_tx });
    let mut catalog = ChannelAdapterCatalog::new();
    catalog.register(build_feishu_runtime_adapter(host));
    catalog
}

pub fn build_channel_runtime(catalog: ChannelAdapterCatalog) -> ChannelRuntimeSupervisor {
    ChannelRuntimeSupervisor::new(catalog)
}

pub fn supported_channel_definitions(
    catalog: &ChannelAdapterCatalog,
) -> Vec<SupportedChannelDefinition> {
    catalog.definitions()
}

pub async fn sync_channel_runtime(state: &AppState) -> Result<(), String> {
    let profile_registry = read_lock(&state.channel_profile_registry_snapshot).clone();
    let desired_profiles = profile_registry
        .channels()
        .iter()
        .filter(|profile| profile.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let mut runtime = state.channel_runtime.lock().await;
    runtime.sync(desired_profiles).map_err(|error| error.to_string())
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
        self.session_manager.submit_turn(session_id, prompt).await.map_err(runtime_error_to_string)
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

fn map_sse_payload(payload: SsePayload) -> Option<ChannelRuntimeEvent> {
    match payload {
        SsePayload::CurrentTurnStarted { session_id, current_turn } => {
            Some(ChannelRuntimeEvent::CurrentTurnStarted {
                session_id,
                current_turn: map_current_turn_snapshot(current_turn),
            })
        }
        SsePayload::Status { session_id, turn_id, status } => Some(ChannelRuntimeEvent::Status {
            session_id,
            turn_id,
            status: map_turn_status(status),
        }),
        SsePayload::Stream { session_id, turn_id, event } => {
            Some(ChannelRuntimeEvent::Stream { session_id, turn_id, event })
        }
        SsePayload::TurnCompleted { session_id, turn_id, turn } => {
            Some(ChannelRuntimeEvent::TurnCompleted { session_id, turn_id, turn })
        }
        SsePayload::Error { session_id, turn_id, message } => {
            Some(ChannelRuntimeEvent::Error { session_id, turn_id, message })
        }
        _ => None,
    }
}

fn map_current_turn_snapshot(snapshot: CurrentTurnSnapshot) -> ChannelCurrentTurnSnapshot {
    ChannelCurrentTurnSnapshot {
        turn_id: snapshot.turn_id,
        started_at_ms: snapshot.started_at_ms,
        user_message: snapshot.user_message,
        status: map_turn_status(snapshot.status),
    }
}

fn map_turn_status(status: TurnStatus) -> ChannelTurnStatus {
    match status {
        TurnStatus::Waiting => ChannelTurnStatus::Waiting,
        TurnStatus::Thinking => ChannelTurnStatus::Thinking,
        TurnStatus::Working => ChannelTurnStatus::Working,
        TurnStatus::Generating => ChannelTurnStatus::Generating,
        TurnStatus::Finishing => ChannelTurnStatus::Finishing,
        TurnStatus::Cancelled => ChannelTurnStatus::Cancelled,
    }
}

fn runtime_error_to_bridge_error(error: RuntimeWorkerError) -> ChannelBridgeError {
    ChannelBridgeError::new(error.message)
}

fn runtime_error_to_string(error: RuntimeWorkerError) -> String {
    error.message
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agent_core::StreamEvent;
    use agent_runtime::{TurnLifecycle, TurnOutcome};
    use channel_bridge::ChannelRuntimeSupervisor;

    use super::*;

    #[test]
    fn map_status_payload_to_feishu_runtime_event() {
        let payload = SsePayload::Status {
            session_id: "s1".into(),
            turn_id: "turn-1".into(),
            status: TurnStatus::Thinking,
        };

        let mapped = map_sse_payload(payload);

        assert!(matches!(
            mapped,
            Some(ChannelRuntimeEvent::Status {
                session_id,
                turn_id,
                status: ChannelTurnStatus::Thinking,
            }) if session_id == "s1" && turn_id == "turn-1"
        ));
    }

    #[test]
    fn map_stream_payload_to_feishu_runtime_event() {
        let payload = SsePayload::Stream {
            session_id: "s1".into(),
            turn_id: "turn-1".into(),
            event: StreamEvent::TextDelta { text: "增量".into() },
        };

        let mapped = map_sse_payload(payload);

        assert!(matches!(
            mapped,
            Some(ChannelRuntimeEvent::Stream { session_id, turn_id, .. })
                if session_id == "s1" && turn_id == "turn-1"
        ));
    }

    #[test]
    fn build_channel_runtime_registers_feishu_adapter() {
        let store = Arc::new(AiaStore::in_memory().expect("memory store"));
        let session_manager = SessionManagerHandle::test_handle();
        let broadcast_tx = tokio::sync::broadcast::channel(8).0;
        let catalog = build_channel_adapter_catalog(store, session_manager, broadcast_tx);

        let _runtime: ChannelRuntimeSupervisor = build_channel_runtime(catalog);
    }

    #[test]
    fn map_turn_completed_payload_to_feishu_runtime_event() {
        let payload = SsePayload::TurnCompleted {
            session_id: "s1".into(),
            turn_id: "turn-1".into(),
            turn: TurnLifecycle {
                turn_id: "turn-1".into(),
                started_at_ms: 1,
                finished_at_ms: 2,
                source_entry_ids: vec![1],
                user_message: "用户问题".into(),
                blocks: vec![agent_runtime::TurnBlock::Assistant { content: "回答".into() }],
                assistant_message: Some("回答".into()),
                thinking: None,
                tool_invocations: vec![],
                usage: None,
                failure_message: None,
                outcome: TurnOutcome::Succeeded,
            },
        };

        let mapped = map_sse_payload(payload);

        assert!(matches!(
            mapped,
            Some(ChannelRuntimeEvent::TurnCompleted { session_id, turn_id, .. })
                if session_id == "s1" && turn_id == "turn-1"
        ));
    }
}
