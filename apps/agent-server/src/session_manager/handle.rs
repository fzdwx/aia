use agent_runtime::{ContextStats, TurnLifecycle};
use agent_store::SessionRecord;
use std::path::{Path, PathBuf};
use async_trait::async_trait;
use channel_bridge::{ChannelBridgeError, ChannelSessionInfo, ChannelSessionService};
use tokio::sync::{mpsc, oneshot};

use crate::runtime_worker::{
    CreateProviderInput, CurrentTurnSnapshot, ProviderInfoSnapshot, RuntimeWorkerError,
    SwitchProviderInput, UpdateProviderInput,
};
use session_tape::SessionProviderBinding;

use super::types::SessionCommand;

#[cfg(test)]
#[path = "../../tests/session_manager/handle/mod.rs"]
mod tests;

#[derive(Clone)]
pub struct SessionManagerHandle {
    pub(super) tx: mpsc::Sender<SessionCommand>,
    pub(super) workspace_root: PathBuf,
}

impl SessionManagerHandle {
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub(super) fn new(tx: mpsc::Sender<SessionCommand>, workspace_root: PathBuf) -> Self {
        Self { tx, workspace_root }
    }

    async fn request<R>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<R, RuntimeWorkerError>>) -> SessionCommand,
    ) -> Result<R, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(build(reply_tx)).await.map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn create_session(
        &self,
        title: Option<String>,
    ) -> Result<SessionRecord, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::CreateSession { title, reply }).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionRecord>, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::ListSessions { reply }).await
    }

    pub async fn delete_session(&self, session_id: String) -> Result<(), RuntimeWorkerError> {
        self.request(|reply| SessionCommand::DeleteSession { session_id, reply }).await
    }

    pub async fn submit_turn(
        &self,
        session_id: String,
        prompt: String,
    ) -> Result<String, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::SubmitTurn { session_id, prompt, reply }).await
    }

    pub async fn cancel_turn(&self, session_id: String) -> Result<bool, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::CancelTurn { session_id, reply }).await
    }

    pub async fn get_history(
        &self,
        session_id: String,
    ) -> Result<Vec<TurnLifecycle>, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::GetHistory { session_id, reply }).await
    }

    pub async fn get_current_turn(
        &self,
        session_id: String,
    ) -> Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::GetCurrentTurn { session_id, reply }).await
    }

    pub async fn get_session_info(
        &self,
        session_id: String,
    ) -> Result<ContextStats, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::GetSessionInfo { session_id, reply }).await
    }

    pub async fn create_handoff(
        &self,
        session_id: String,
        name: String,
        summary: String,
    ) -> Result<u64, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::CreateHandoff { session_id, name, summary, reply })
            .await
    }

    pub async fn auto_compress_session(
        &self,
        session_id: String,
    ) -> Result<bool, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::AutoCompressSession { session_id, reply }).await
    }

    pub async fn get_session_settings(
        &self,
        session_id: String,
    ) -> Result<SessionProviderBinding, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::GetSessionSettings { session_id, reply }).await
    }

    pub async fn update_session_settings(
        &self,
        session_id: String,
        provider_binding: SessionProviderBinding,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::UpdateSessionSettings {
            session_id,
            provider_binding,
            reply,
        })
        .await
    }

    pub async fn create_provider(
        &self,
        input: CreateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        self.request(|reply| SessionCommand::CreateProvider { input, reply }).await
    }

    pub async fn update_provider(
        &self,
        name: String,
        input: UpdateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        self.request(|reply| SessionCommand::UpdateProvider { name, input, reply }).await
    }

    pub async fn delete_provider(&self, name: String) -> Result<(), RuntimeWorkerError> {
        self.request(|reply| SessionCommand::DeleteProvider { name, reply }).await
    }

    pub async fn switch_provider(
        &self,
        input: SwitchProviderInput,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        self.request(|reply| SessionCommand::SwitchProvider { input, reply }).await
    }
}

#[async_trait]
impl ChannelSessionService for SessionManagerHandle {
    async fn session_exists(&self, session_id: &str) -> Result<bool, ChannelBridgeError> {
        Ok(self.get_session_info(session_id.to_string()).await.is_ok())
    }

    async fn create_session(&self, title: String) -> Result<String, ChannelBridgeError> {
        SessionManagerHandle::create_session(self, Some(title))
            .await
            .map(|session| session.id)
            .map_err(|error| ChannelBridgeError::new(error.message))
    }

    async fn session_info(
        &self,
        session_id: &str,
    ) -> Result<ChannelSessionInfo, ChannelBridgeError> {
        let stats = self
            .get_session_info(session_id.to_string())
            .await
            .map_err(|error| ChannelBridgeError::new(error.message))?;
        Ok(ChannelSessionInfo { pressure_ratio: stats.pressure_ratio })
    }

    async fn auto_compress_session(&self, session_id: &str) -> Result<bool, ChannelBridgeError> {
        SessionManagerHandle::auto_compress_session(self, session_id.to_string())
            .await
            .map_err(|error| ChannelBridgeError::new(error.message))
    }
}
