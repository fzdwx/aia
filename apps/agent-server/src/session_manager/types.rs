use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use agent_core::ToolRegistry;
use agent_runtime::{AgentRuntime, ContextStats, RuntimeSubscriberId, TurnLifecycle};
use agent_store::{AiaStore, SessionRecord};
use provider_registry::ProviderRegistry;
use session_tape::SessionProviderBinding;
use tokio::sync::{broadcast, oneshot};

use crate::{
    model::ServerModel,
    runtime_worker::{
        CreateProviderInput, CurrentTurnSnapshot, ProviderInfoSnapshot, RunningTurnHandle,
        RuntimeWorkerError, SwitchProviderInput, UpdateProviderInput,
    },
    sse::SsePayload,
};

pub(crate) type SessionId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SlotStatus {
    Idle,
    Running,
}

pub(crate) struct SessionSlot {
    pub(crate) runtime: Option<AgentRuntime<ServerModel, ToolRegistry>>,
    pub(crate) subscriber: RuntimeSubscriberId,
    pub(crate) session_path: PathBuf,
    pub(crate) history: Arc<RwLock<Vec<TurnLifecycle>>>,
    pub(crate) current_turn: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    pub(crate) context_stats: Arc<RwLock<ContextStats>>,
    pub(crate) running_turn: Option<RunningTurnHandle>,
    pub(crate) pending_provider_binding: Option<SessionProviderBinding>,
    pub(crate) status: SlotStatus,
}

pub(crate) struct RuntimeReturn {
    pub(crate) session_id: SessionId,
    pub(crate) runtime: AgentRuntime<ServerModel, ToolRegistry>,
    pub(crate) subscriber: RuntimeSubscriberId,
}

pub(crate) enum SessionCommand {
    CreateSession {
        title: Option<String>,
        reply: oneshot::Sender<Result<SessionRecord, RuntimeWorkerError>>,
    },
    DeleteSession {
        session_id: SessionId,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },
    SubmitTurn {
        session_id: SessionId,
        prompt: String,
        reply: oneshot::Sender<Result<String, RuntimeWorkerError>>,
    },
    CancelTurn {
        session_id: SessionId,
        reply: oneshot::Sender<Result<bool, RuntimeWorkerError>>,
    },
    GetHistory {
        session_id: SessionId,
        reply: oneshot::Sender<Result<Vec<TurnLifecycle>, RuntimeWorkerError>>,
    },
    GetCurrentTurn {
        session_id: SessionId,
        reply: oneshot::Sender<Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError>>,
    },
    GetSessionInfo {
        session_id: SessionId,
        reply: oneshot::Sender<Result<ContextStats, RuntimeWorkerError>>,
    },
    CreateHandoff {
        session_id: SessionId,
        name: String,
        summary: String,
        reply: oneshot::Sender<Result<u64, RuntimeWorkerError>>,
    },
    AutoCompressSession {
        session_id: SessionId,
        reply: oneshot::Sender<Result<bool, RuntimeWorkerError>>,
    },
    CreateProvider {
        input: CreateProviderInput,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },
    UpdateProvider {
        name: String,
        input: UpdateProviderInput,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },
    DeleteProvider {
        name: String,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },
    SwitchProvider {
        input: SwitchProviderInput,
        reply: oneshot::Sender<Result<ProviderInfoSnapshot, RuntimeWorkerError>>,
    },
}

pub struct SessionManagerConfig {
    pub sessions_dir: PathBuf,
    pub store: Arc<AiaStore>,
    pub registry: ProviderRegistry,
    pub provider_registry_path: PathBuf,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub workspace_root: PathBuf,
    pub user_agent: String,
}

pub(crate) fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|poisoned| poisoned.into_inner())
}
