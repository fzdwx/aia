use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use agent_core::{QuestionRequest, QuestionResult, RequestTimeoutConfig, ToolRegistry};
use agent_runtime::{AgentRuntime, ContextStats, RuntimeHooks, RuntimeSubscriberId, TurnLifecycle};
use agent_store::{AiaStore, SessionRecord};
use provider_registry::ProviderRegistry;
use session_tape::SessionProviderBinding;
use tokio::sync::oneshot as tokio_oneshot;
use tokio::sync::{broadcast, mpsc, oneshot};

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

pub(crate) enum SlotExecutionState {
    Idle {
        runtime: AgentRuntime<ServerModel, ToolRegistry>,
        subscriber: RuntimeSubscriberId,
    },
    Running {
        subscriber: RuntimeSubscriberId,
        running_turn: RunningTurnHandle,
        pending_provider_binding: Option<SessionProviderBinding>,
    },
    Transitioning,
}

pub(crate) struct SessionSlot {
    pub(crate) session_path: PathBuf,
    pub(crate) provider_binding: SessionProviderBinding,
    pub(crate) history: Arc<RwLock<Vec<TurnLifecycle>>>,
    pub(crate) current_turn: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    pub(crate) context_stats: Arc<RwLock<ContextStats>>,
    pub(crate) execution: SlotExecutionState,
    pub(crate) pending_question_waiters: HashMap<String, tokio_oneshot::Sender<QuestionResult>>,
}

impl SessionSlot {
    pub(crate) fn idle(
        session_path: PathBuf,
        provider_binding: SessionProviderBinding,
        history: Arc<RwLock<Vec<TurnLifecycle>>>,
        current_turn: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
        context_stats: Arc<RwLock<ContextStats>>,
        runtime: AgentRuntime<ServerModel, ToolRegistry>,
        subscriber: RuntimeSubscriberId,
    ) -> Self {
        Self {
            session_path,
            provider_binding,
            history,
            current_turn,
            context_stats,
            execution: SlotExecutionState::Idle { runtime, subscriber },
            pending_question_waiters: HashMap::new(),
        }
    }

    pub(crate) fn status(&self) -> SlotStatus {
        match self.execution {
            SlotExecutionState::Idle { .. } => SlotStatus::Idle,
            SlotExecutionState::Running { .. } | SlotExecutionState::Transitioning => {
                SlotStatus::Running
            }
        }
    }

    pub(crate) fn runtime(&self) -> Option<&AgentRuntime<ServerModel, ToolRegistry>> {
        match &self.execution {
            SlotExecutionState::Idle { runtime, .. } => Some(runtime),
            SlotExecutionState::Running { .. } | SlotExecutionState::Transitioning => None,
        }
    }

    pub(crate) fn runtime_mut(&mut self) -> Option<&mut AgentRuntime<ServerModel, ToolRegistry>> {
        match &mut self.execution {
            SlotExecutionState::Idle { runtime, .. } => Some(runtime),
            SlotExecutionState::Running { .. } | SlotExecutionState::Transitioning => None,
        }
    }

    pub(crate) fn subscriber(&self) -> RuntimeSubscriberId {
        match &self.execution {
            SlotExecutionState::Idle { subscriber, .. }
            | SlotExecutionState::Running { subscriber, .. } => *subscriber,
            SlotExecutionState::Transitioning => 0,
        }
    }

    pub(crate) fn running_turn(&self) -> Option<&RunningTurnHandle> {
        match &self.execution {
            SlotExecutionState::Running { running_turn, .. } => Some(running_turn),
            SlotExecutionState::Idle { .. } | SlotExecutionState::Transitioning => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_provider_binding(&self) -> Option<&SessionProviderBinding> {
        match &self.execution {
            SlotExecutionState::Running { pending_provider_binding, .. } => {
                pending_provider_binding.as_ref()
            }
            SlotExecutionState::Idle { .. } | SlotExecutionState::Transitioning => None,
        }
    }

    pub(crate) fn take_pending_provider_binding(&mut self) -> Option<SessionProviderBinding> {
        match &mut self.execution {
            SlotExecutionState::Running { pending_provider_binding, .. } => {
                pending_provider_binding.take()
            }
            SlotExecutionState::Idle { .. } | SlotExecutionState::Transitioning => None,
        }
    }

    pub(crate) fn replace_pending_provider_binding(
        &mut self,
        binding: Option<SessionProviderBinding>,
    ) -> Result<(), RuntimeWorkerError> {
        match &mut self.execution {
            SlotExecutionState::Running { pending_provider_binding, .. } => {
                *pending_provider_binding = binding;
                Ok(())
            }
            SlotExecutionState::Idle { .. } => {
                Err(RuntimeWorkerError::bad_request("session is not currently running"))
            }
            SlotExecutionState::Transitioning => {
                Err(RuntimeWorkerError::internal("session state transition in progress"))
            }
        }
    }

    pub(crate) fn begin_turn(
        &mut self,
    ) -> Result<
        (AgentRuntime<ServerModel, ToolRegistry>, RuntimeSubscriberId, RunningTurnHandle),
        RuntimeWorkerError,
    > {
        let state = std::mem::replace(&mut self.execution, SlotExecutionState::Transitioning);
        match state {
            SlotExecutionState::Idle { runtime, subscriber } => {
                let running_turn = RunningTurnHandle { control: runtime.turn_control() };
                self.execution = SlotExecutionState::Running {
                    subscriber,
                    running_turn: running_turn.clone(),
                    pending_provider_binding: None,
                };
                Ok((runtime, subscriber, running_turn))
            }
            state @ SlotExecutionState::Running { .. } => {
                self.execution = state;
                Err(RuntimeWorkerError::bad_request("a turn is already running in this session"))
            }
            SlotExecutionState::Transitioning => {
                Err(RuntimeWorkerError::internal("session state transition in progress"))
            }
        }
    }

    pub(crate) fn finish_turn(
        &mut self,
        runtime: AgentRuntime<ServerModel, ToolRegistry>,
        subscriber: RuntimeSubscriberId,
    ) -> Result<(), RuntimeWorkerError> {
        let state = std::mem::replace(&mut self.execution, SlotExecutionState::Transitioning);
        match state {
            SlotExecutionState::Running { .. } => {
                self.execution = SlotExecutionState::Idle { runtime, subscriber };
                Ok(())
            }
            state @ SlotExecutionState::Idle { .. } => {
                self.execution = state;
                Err(RuntimeWorkerError::internal("session turn completed while idle"))
            }
            SlotExecutionState::Transitioning => {
                Err(RuntimeWorkerError::internal("session state transition in progress"))
            }
        }
    }

    pub(crate) fn insert_pending_question_waiter(
        &mut self,
        request_id: String,
        sender: tokio_oneshot::Sender<QuestionResult>,
    ) {
        self.pending_question_waiters.insert(request_id, sender);
    }

    pub(crate) fn remove_pending_question_waiter(
        &mut self,
        request_id: &str,
    ) -> Option<tokio_oneshot::Sender<QuestionResult>> {
        self.pending_question_waiters.remove(request_id)
    }
}

pub(crate) struct RuntimeReturn {
    pub(crate) session_id: SessionId,
    pub(crate) runtime: AgentRuntime<ServerModel, ToolRegistry>,
    pub(crate) subscriber: RuntimeSubscriberId,
}

pub(crate) enum SessionCommand {
    ListSessions {
        reply: oneshot::Sender<Result<Vec<SessionRecord>, RuntimeWorkerError>>,
    },
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
    GetSessionSettings {
        session_id: SessionId,
        reply: oneshot::Sender<Result<SessionProviderBinding, RuntimeWorkerError>>,
    },
    GetPendingQuestion {
        session_id: SessionId,
        reply: oneshot::Sender<Result<Option<QuestionRequest>, RuntimeWorkerError>>,
    },
    AskQuestion {
        session_id: SessionId,
        request: QuestionRequest,
        reply: oneshot::Sender<Result<QuestionResult, RuntimeWorkerError>>,
    },
    ResolvePendingQuestion {
        session_id: SessionId,
        result: QuestionResult,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },
    CancelPendingQuestion {
        session_id: SessionId,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },
    UpdateSessionSettings {
        session_id: SessionId,
        provider_binding: SessionProviderBinding,
        reply: oneshot::Sender<Result<ProviderInfoSnapshot, RuntimeWorkerError>>,
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
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub workspace_root: PathBuf,
    pub user_agent: String,
    pub request_timeout: RequestTimeoutConfig,
    pub system_prompt: Option<String>,
    pub runtime_hooks: RuntimeHooks,
    pub runtime_tool_host: Arc<RuntimeToolHost>,
}

pub struct RuntimeToolHost {
    pub(crate) tx: mpsc::Sender<SessionCommand>,
}

pub(crate) fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|poisoned| poisoned.into_inner())
}
