use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use agent_core::{
    ModelIdentity, PromptCacheConfig, PromptCacheRetention as RuntimePromptCacheRetention,
    StreamEvent, ToolRegistry,
};
use agent_runtime::{AgentRuntime, ContextStats, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use agent_store::{AiaStore, LlmTraceStore, SessionRecord, generate_session_id, iso8601_now};
use builtin_tools::build_tool_registry;
use provider_registry::ProviderRegistry;
use session_tape::{SessionProviderBinding, SessionTape};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    sse::{SsePayload, TurnStatus},
};

// Re-export types that routes and tests still need
use crate::runtime_worker::rebuild_session_snapshots_from_tape;
pub use crate::runtime_worker::{
    CreateProviderInput, CurrentToolOutput, CurrentTurnBlock, CurrentTurnSnapshot,
    ProviderInfoSnapshot, RunningTurnHandle, RuntimeWorkerError, SwitchProviderInput,
    UpdateProviderInput,
};

pub type SessionId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
enum SlotStatus {
    Idle,
    Running,
}

struct SessionSlot {
    runtime: Option<AgentRuntime<ServerModel, ToolRegistry>>,
    subscriber: RuntimeSubscriberId,
    session_path: PathBuf,
    history: Arc<RwLock<Vec<TurnLifecycle>>>,
    current_turn: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    running_turn: Option<RunningTurnHandle>,
    status: SlotStatus,
}

/// Sent back from spawn_blocking when a turn completes.
struct RuntimeReturn {
    session_id: SessionId,
    runtime: AgentRuntime<ServerModel, ToolRegistry>,
    subscriber: RuntimeSubscriberId,
}

enum SessionCommand {
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
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
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

#[derive(Clone)]
pub struct SessionManagerHandle {
    tx: mpsc::Sender<SessionCommand>,
}

impl SessionManagerHandle {
    pub async fn create_session(
        &self,
        title: Option<String>,
    ) -> Result<SessionRecord, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::CreateSession { title, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn delete_session(&self, session_id: String) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::DeleteSession { session_id, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub fn submit_turn(
        &self,
        session_id: String,
        prompt: String,
    ) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, _reply_rx) = oneshot::channel();
        self.tx
            .try_send(SessionCommand::SubmitTurn { session_id, prompt, reply: reply_tx })
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        Ok(())
    }

    pub async fn cancel_turn(&self, session_id: String) -> Result<bool, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::CancelTurn { session_id, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn get_history(
        &self,
        session_id: String,
    ) -> Result<Vec<TurnLifecycle>, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::GetHistory { session_id, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn get_current_turn(
        &self,
        session_id: String,
    ) -> Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::GetCurrentTurn { session_id, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn get_session_info(
        &self,
        session_id: String,
    ) -> Result<ContextStats, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::GetSessionInfo { session_id, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn create_handoff(
        &self,
        session_id: String,
        name: String,
        summary: String,
    ) -> Result<u64, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::CreateHandoff { session_id, name, summary, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn create_provider(
        &self,
        input: CreateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::CreateProvider { input, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn update_provider(
        &self,
        name: String,
        input: UpdateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::UpdateProvider { name, input, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn delete_provider(&self, name: String) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::DeleteProvider { name, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn switch_provider(
        &self,
        input: SwitchProviderInput,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::SwitchProvider { input, reply: reply_tx })
            .await
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }
}

pub struct SessionManagerConfig {
    pub sessions_dir: PathBuf,
    pub store: Arc<AiaStore>,
    pub registry: ProviderRegistry,
    pub store_path: PathBuf,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub workspace_root: PathBuf,
    pub user_agent: String,
}

pub fn spawn_session_manager(config: SessionManagerConfig) -> SessionManagerHandle {
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(session_manager_loop(rx, config));
    SessionManagerHandle { tx }
}

pub(crate) fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(crate) fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

async fn session_manager_loop(
    mut rx: mpsc::Receiver<SessionCommand>,
    mut config: SessionManagerConfig,
) {
    let mut slots: HashMap<SessionId, SessionSlot> = HashMap::new();
    let (return_tx, mut return_rx) = mpsc::channel::<RuntimeReturn>(64);

    // Load existing sessions from DB and hydrate slots
    if let Ok(records) = config.store.list_sessions() {
        for record in records {
            if let Ok(slot) = create_slot_for_session(&record.id, &config) {
                slots.insert(record.id, slot);
            }
        }
    }

    loop {
        tokio::select! {
            Some(command) = rx.recv() => {
                match command {
                    SessionCommand::CreateSession { title, reply } => {
                        let result = handle_create_session(&mut slots, &config, title);
                        let _ = reply.send(result);
                    }
                    SessionCommand::DeleteSession { session_id, reply } => {
                        let result = handle_delete_session(&mut slots, &config, &session_id);
                        let _ = reply.send(result);
                    }
                    SessionCommand::SubmitTurn { session_id, prompt, reply } => {
                        let result = handle_submit_turn(&mut slots, &config, &return_tx, &session_id, prompt);
                        let _ = reply.send(result);
                    }
                    SessionCommand::CancelTurn { session_id, reply } => {
                        let result = handle_cancel_turn(&mut slots, &config, &session_id);
                        let _ = reply.send(result);
                    }
                    SessionCommand::GetHistory { session_id, reply } => {
                        let result = handle_get_history(&slots, &session_id);
                        let _ = reply.send(result);
                    }
                    SessionCommand::GetCurrentTurn { session_id, reply } => {
                        let result = handle_get_current_turn(&slots, &session_id);
                        let _ = reply.send(result);
                    }
                    SessionCommand::GetSessionInfo { session_id, reply } => {
                        let result = handle_get_session_info(&slots, &session_id);
                        let _ = reply.send(result);
                    }
                    SessionCommand::CreateHandoff { session_id, name, summary, reply } => {
                        let result = handle_create_handoff(&mut slots, &session_id, name, summary);
                        let _ = reply.send(result);
                    }
                    SessionCommand::CreateProvider { input, reply } => {
                        let result = handle_create_provider(&mut slots, &mut config, input);
                        let _ = reply.send(result);
                    }
                    SessionCommand::UpdateProvider { name, input, reply } => {
                        let result = handle_update_provider(&mut slots, &mut config, name, input);
                        let _ = reply.send(result);
                    }
                    SessionCommand::DeleteProvider { name, reply } => {
                        let result = handle_delete_provider(&mut slots, &mut config, name);
                        let _ = reply.send(result);
                    }
                    SessionCommand::SwitchProvider { input, reply } => {
                        let result = handle_switch_provider(&mut slots, &mut config, input);
                        let _ = reply.send(result);
                    }
                }
            }
            Some(ret) = return_rx.recv() => {
                // Put runtime back into the slot after turn completion
                if let Some(slot) = slots.get_mut(&ret.session_id) {
                    slot.runtime = Some(ret.runtime);
                    slot.subscriber = ret.subscriber;
                    slot.running_turn = None;
                    slot.status = SlotStatus::Idle;
                }
            }
            else => break,
        }
    }
}

fn create_slot_for_session(
    session_id: &str,
    config: &SessionManagerConfig,
) -> Result<SessionSlot, RuntimeWorkerError> {
    let session_path = config.sessions_dir.join(format!("{session_id}.jsonl"));
    let tape = SessionTape::load_jsonl_or_default(&session_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("tape load failed: {e}")))?;

    let selection = choose_provider_for_tape(&config.registry, &tape);
    let prompt_cache = prompt_cache_for_selection(&selection, session_id);
    let (identity, model) = build_model_from_selection(selection, Some(config.store.clone()))
        .map_err(|e| RuntimeWorkerError::internal(e.to_string()))?;

    let session_append_path = session_path.clone();
    let workspace_root = config.workspace_root.clone();

    let mut runtime = AgentRuntime::with_tape(model, build_tool_registry(), identity, tape)
        .with_instructions(format!(
            "你是 aia 的助手。给出清晰、结构化的答案。\n\n{}",
            agent_prompts::context_contract(
                agent_prompts::AGENT_HANDOFF_THRESHOLD,
                agent_prompts::AUTO_COMPRESSION_THRESHOLD,
            ),
        ))
        .with_user_agent(config.user_agent.clone())
        .with_workspace_root(workspace_root)
        .with_tape_entry_listener(move |entry| {
            SessionTape::append_jsonl_entry(&session_append_path, entry)
        })
        .with_max_tool_calls_per_turn(100000);
    if let Some(prompt_cache) = prompt_cache {
        runtime = runtime.with_prompt_cache(prompt_cache);
    }

    let subscriber = runtime.subscribe();
    let snapshots = rebuild_session_snapshots_from_tape(runtime.tape());

    Ok(SessionSlot {
        runtime: Some(runtime),
        subscriber,
        session_path,
        history: Arc::new(RwLock::new(snapshots.history)),
        current_turn: Arc::new(RwLock::new(snapshots.current_turn)),
        running_turn: None,
        status: SlotStatus::Idle,
    })
}

fn choose_provider_for_tape(
    registry: &ProviderRegistry,
    tape: &SessionTape,
) -> ProviderLaunchChoice {
    if let Some(binding) = tape.latest_provider_binding() {
        match binding {
            SessionProviderBinding::Bootstrap => return ProviderLaunchChoice::Bootstrap,
            SessionProviderBinding::Provider { name, model, base_url, protocol } => {
                if let Some(profile) = registry.providers().iter().find(|provider| {
                    provider.name == name
                        && provider.has_model(&model)
                        && provider.base_url == base_url
                        && provider.kind.protocol_name() == protocol.as_str()
                }) {
                    return ProviderLaunchChoice::OpenAi(profile.clone());
                }
            }
        }
    }

    registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap)
}

fn handle_create_session(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    title: Option<String>,
) -> Result<SessionRecord, RuntimeWorkerError> {
    let session_id = generate_session_id();
    let now = iso8601_now();
    let title = title.unwrap_or_else(|| "New session".to_string());

    let model_name =
        config.provider_info_snapshot.read().map(|info| info.model.clone()).unwrap_or_default();

    let record = SessionRecord {
        id: session_id.clone(),
        title: title.clone(),
        created_at: now.clone(),
        updated_at: now,
        model: model_name,
    };

    config
        .store
        .create_session(&record)
        .map_err(|e| RuntimeWorkerError::internal(format!("session db insert failed: {e}")))?;

    let slot = create_slot_for_session(&session_id, config)?;
    slots.insert(session_id.clone(), slot);

    let _ = config
        .broadcast_tx
        .send(SsePayload::SessionCreated { session_id: session_id.clone(), title });

    Ok(record)
}

fn handle_delete_session(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    session_id: &str,
) -> Result<(), RuntimeWorkerError> {
    if let Some(slot) = slots.get(session_id)
        && slot.status == SlotStatus::Running
    {
        return Err(RuntimeWorkerError::bad_request(
            "cannot delete a session while a turn is running",
        ));
    }

    slots.remove(session_id);

    let session_path = config.sessions_dir.join(format!("{session_id}.jsonl"));
    if session_path.exists() {
        std::fs::remove_file(&session_path)
            .map_err(|e| RuntimeWorkerError::internal(format!("jsonl delete failed: {e}")))?;
    }

    config
        .store
        .delete_session(session_id)
        .map_err(|e| RuntimeWorkerError::internal(format!("session db delete failed: {e}")))?;

    let _ =
        config.broadcast_tx.send(SsePayload::SessionDeleted { session_id: session_id.to_string() });

    Ok(())
}

fn handle_submit_turn(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    return_tx: &mpsc::Sender<RuntimeReturn>,
    session_id: &str,
    prompt: String,
) -> Result<(), RuntimeWorkerError> {
    let slot = slots
        .get_mut(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;

    if slot.status == SlotStatus::Running {
        return Err(RuntimeWorkerError::bad_request("a turn is already running in this session"));
    }

    let mut runtime =
        slot.runtime.take().ok_or_else(|| RuntimeWorkerError::internal("runtime not available"))?;
    let subscriber = slot.subscriber;
    let turn_control = runtime.turn_control();
    slot.running_turn = Some(RunningTurnHandle { control: turn_control.clone() });
    slot.status = SlotStatus::Running;

    let _ = config.store.update_session(session_id, None, None);

    // Initialize current turn snapshot
    *write_lock(&slot.current_turn) = Some(CurrentTurnSnapshot {
        started_at_ms: now_timestamp_ms(),
        user_message: prompt.clone(),
        status: TurnStatus::Waiting,
        blocks: Vec::new(),
    });

    let broadcast_tx = config.broadcast_tx.clone();
    let current_turn_snapshot = slot.current_turn.clone();
    let history_snapshot = slot.history.clone();
    let trace_store = config.store.clone();
    let sid = session_id.to_string();
    let return_tx = return_tx.clone();
    let turn_control = turn_control.clone();

    let _ = broadcast_tx
        .send(SsePayload::Status { session_id: sid.clone(), status: TurnStatus::Waiting });

    tokio::task::spawn_blocking(move || {
        let mut current_status = CurrentStatusInner::Waiting;
        let btx = broadcast_tx.clone();
        let sid2 = sid.clone();
        let cts = current_turn_snapshot.clone();

        let result = runtime.handle_turn_streaming_with_control(prompt, turn_control, |event| {
            let new_status = match &event {
                StreamEvent::ThinkingDelta { .. } => CurrentStatusInner::Thinking,
                StreamEvent::TextDelta { .. } => CurrentStatusInner::Generating,
                StreamEvent::ToolCallDetected { .. } => current_status.clone(),
                StreamEvent::ToolCallStarted { .. } => CurrentStatusInner::Working,
                StreamEvent::ToolOutputDelta { .. } => CurrentStatusInner::Working,
                _ => current_status.clone(),
            };

            if new_status != current_status {
                current_status = new_status.clone();
                update_current_turn_status(&cts, new_status.to_turn_status());
                let _ = btx.send(SsePayload::Status {
                    session_id: sid2.clone(),
                    status: new_status.to_turn_status(),
                });
            }

            update_current_turn_from_stream(&cts, &event);
            let _ = btx.send(SsePayload::Stream { session_id: sid2.clone(), event });
        });

        match result {
            Ok(_) => {
                let events = runtime.collect_events(subscriber).unwrap_or_default();
                let turn = broadcast_runtime_events_with_session(events, &broadcast_tx, &sid);
                if let Some(turn) = turn {
                    persist_tool_trace_spans(&turn, trace_store.as_ref());
                    write_lock(&history_snapshot).push(turn.clone());
                    let _ = broadcast_tx
                        .send(SsePayload::TurnCompleted { session_id: sid.clone(), turn });
                }
                *write_lock(&current_turn_snapshot) = None;
            }
            Err(error) => {
                let was_cancelled = error.is_cancelled();
                let events = runtime.collect_events(subscriber).unwrap_or_default();
                let turn = broadcast_runtime_events_with_session(events, &broadcast_tx, &sid);
                if let Some(turn) = turn {
                    persist_tool_trace_spans(&turn, trace_store.as_ref());
                    write_lock(&history_snapshot).push(turn.clone());
                    let _ = broadcast_tx
                        .send(SsePayload::TurnCompleted { session_id: sid.clone(), turn });
                }
                if was_cancelled {
                    update_current_turn_status(&current_turn_snapshot, TurnStatus::Cancelled);
                    let _ = broadcast_tx.send(SsePayload::Status {
                        session_id: sid.clone(),
                        status: TurnStatus::Cancelled,
                    });
                    let _ =
                        broadcast_tx.send(SsePayload::TurnCancelled { session_id: sid.clone() });
                }
                *write_lock(&current_turn_snapshot) = None;
                let _ = broadcast_tx.send(SsePayload::Error {
                    session_id: sid.clone(),
                    message: error.to_string(),
                });
            }
        }

        // Return runtime to the session manager
        let _ = return_tx.blocking_send(RuntimeReturn { session_id: sid, runtime, subscriber });
    });

    Ok(())
}

fn handle_cancel_turn(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    session_id: &str,
) -> Result<bool, RuntimeWorkerError> {
    let slot = slots
        .get_mut(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;

    if slot.status != SlotStatus::Running {
        return Ok(false);
    }

    let Some(running_turn) = slot.running_turn.as_ref() else {
        return Err(RuntimeWorkerError::internal("running turn handle missing"));
    };

    running_turn.control.cancel();
    update_current_turn_status(&slot.current_turn, TurnStatus::Cancelled);
    let _ = config.broadcast_tx.send(SsePayload::Status {
        session_id: session_id.to_string(),
        status: TurnStatus::Cancelled,
    });
    let _ =
        config.broadcast_tx.send(SsePayload::TurnCancelled { session_id: session_id.to_string() });
    Ok(true)
}

fn handle_get_history(
    slots: &HashMap<SessionId, SessionSlot>,
    session_id: &str,
) -> Result<Vec<TurnLifecycle>, RuntimeWorkerError> {
    let slot = slots
        .get(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    Ok(read_lock(&slot.history).clone())
}

fn handle_get_current_turn(
    slots: &HashMap<SessionId, SessionSlot>,
    session_id: &str,
) -> Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError> {
    let slot = slots
        .get(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    Ok(read_lock(&slot.current_turn).clone())
}

fn handle_get_session_info(
    slots: &HashMap<SessionId, SessionSlot>,
    session_id: &str,
) -> Result<ContextStats, RuntimeWorkerError> {
    let slot = slots
        .get(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    let runtime = slot
        .runtime
        .as_ref()
        .ok_or_else(|| RuntimeWorkerError::bad_request("session is currently running a turn"))?;
    Ok(runtime.context_stats())
}

fn handle_create_handoff(
    slots: &mut HashMap<SessionId, SessionSlot>,
    session_id: &str,
    name: String,
    summary: String,
) -> Result<u64, RuntimeWorkerError> {
    let slot = slots
        .get_mut(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    let runtime = slot
        .runtime
        .as_mut()
        .ok_or_else(|| RuntimeWorkerError::bad_request("session is currently running a turn"))?;

    let handoff = runtime.handoff(
        &name,
        serde_json::json!({
            "phase": name,
            "summary": summary,
            "next_steps": [],
            "source_entry_ids": [],
            "owner": "user"
        }),
    );
    runtime
        .tape()
        .save_jsonl(&slot.session_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("session save failed: {e}")))?;
    Ok(handoff.anchor.entry_id)
}

fn handle_create_provider(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &mut SessionManagerConfig,
    input: CreateProviderInput,
) -> Result<(), RuntimeWorkerError> {
    let active_model = input.active_model.or_else(|| input.models.first().map(|m| m.id.clone()));
    let mut candidate_registry = config.registry.clone();
    candidate_registry.upsert(provider_registry::ProviderProfile {
        name: input.name,
        kind: input.kind,
        base_url: input.base_url,
        api_key: input.api_key,
        models: input.models,
        active_model,
    });
    sync_all_runtimes_to_registry(slots, config, candidate_registry).map(|_| ())
}

fn handle_update_provider(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &mut SessionManagerConfig,
    name: String,
    input: UpdateProviderInput,
) -> Result<(), RuntimeWorkerError> {
    let profile = config
        .registry
        .providers()
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("provider 不存在：{name}")))?;

    let updated = provider_registry::ProviderProfile {
        name: name.clone(),
        kind: input.kind.unwrap_or(profile.kind),
        base_url: input.base_url.unwrap_or(profile.base_url),
        api_key: input.api_key.unwrap_or(profile.api_key),
        models: input.models.unwrap_or(profile.models),
        active_model: input.active_model.or(profile.active_model),
    };

    let mut candidate_registry = config.registry.clone();
    candidate_registry.upsert(updated);
    sync_all_runtimes_to_registry(slots, config, candidate_registry).map(|_| ())
}

fn handle_delete_provider(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &mut SessionManagerConfig,
    name: String,
) -> Result<(), RuntimeWorkerError> {
    let mut candidate_registry = config.registry.clone();
    candidate_registry.remove(&name).map_err(|e| RuntimeWorkerError::not_found(e.to_string()))?;
    sync_all_runtimes_to_registry(slots, config, candidate_registry).map(|_| ())
}

fn handle_switch_provider(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &mut SessionManagerConfig,
    input: SwitchProviderInput,
) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
    let mut profile =
        config.registry.providers().iter().find(|p| p.name == input.name).cloned().ok_or_else(
            || RuntimeWorkerError::not_found(format!("provider 不存在：{}", input.name)),
        )?;

    if let Some(model_id) = &input.model_id {
        if !profile.has_model(model_id) {
            return Err(RuntimeWorkerError::bad_request(format!("模型不存在：{model_id}")));
        }
        profile.active_model = Some(model_id.to_string());
    }

    let mut candidate_registry = config.registry.clone();
    candidate_registry.upsert(profile);
    candidate_registry
        .set_active(&input.name)
        .map_err(|e| RuntimeWorkerError::bad_request(e.to_string()))?;
    sync_all_runtimes_to_registry(slots, config, candidate_registry)
}

fn sync_all_runtimes_to_registry(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &mut SessionManagerConfig,
    candidate_registry: ProviderRegistry,
) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
    let (info, identity, _model, binding) =
        prepare_runtime_sync(&candidate_registry, Some(config.store.clone()))?;

    candidate_registry
        .save(&config.store_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("provider registry save failed: {e}")))?;

    // Update all idle sessions
    for (session_id, slot) in slots.iter_mut() {
        if let Some(runtime) = slot.runtime.as_mut() {
            let mut candidate_tape = runtime.tape().clone();
            candidate_tape.bind_provider(binding.clone());
            candidate_tape
                .save_jsonl(&slot.session_path)
                .map_err(|e| RuntimeWorkerError::internal(format!("session save failed: {e}")))?;

            let selection = candidate_registry
                .active_provider()
                .cloned()
                .map(ProviderLaunchChoice::OpenAi)
                .unwrap_or(ProviderLaunchChoice::Bootstrap);
            let (_, new_model) = build_model_from_selection(selection, Some(config.store.clone()))
                .map_err(|e| RuntimeWorkerError::internal(e.to_string()))?;
            let prompt_cache = prompt_cache_for_registry(&candidate_registry, session_id);

            runtime.replace_model(new_model, identity.clone());
            runtime.set_prompt_cache(prompt_cache);
            *runtime.tape_mut() = candidate_tape;
        }
    }

    config.registry = candidate_registry;
    *write_lock(&config.provider_registry_snapshot) = config.registry.clone();
    *write_lock(&config.provider_info_snapshot) = info.clone();

    Ok(info)
}

fn prepare_runtime_sync(
    registry: &ProviderRegistry,
    trace_store: Option<Arc<dyn LlmTraceStore>>,
) -> Result<
    (ProviderInfoSnapshot, ModelIdentity, ServerModel, SessionProviderBinding),
    RuntimeWorkerError,
> {
    let selection = registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap);

    let (identity, model) = build_model_from_selection(selection, trace_store)
        .map_err(|e| RuntimeWorkerError::internal(e.to_string()))?;

    let binding = match registry.active_provider() {
        Some(profile) => SessionProviderBinding::Provider {
            name: profile.name.clone(),
            model: profile.active_model_id().unwrap_or("").to_string(),
            base_url: profile.base_url.clone(),
            protocol: profile.kind.protocol_name().to_string(),
        },
        None => SessionProviderBinding::Bootstrap,
    };

    let info = ProviderInfoSnapshot::from_identity(&identity);
    Ok((info, identity, model, binding))
}

fn prompt_cache_for_registry(
    registry: &ProviderRegistry,
    session_id: &str,
) -> Option<PromptCacheConfig> {
    let selection = registry.active_provider().cloned().map(ProviderLaunchChoice::OpenAi)?;
    prompt_cache_for_selection(&selection, session_id)
}

fn prompt_cache_for_selection(
    selection: &ProviderLaunchChoice,
    session_id: &str,
) -> Option<PromptCacheConfig> {
    let ProviderLaunchChoice::OpenAi(profile) = selection else {
        return None;
    };
    let model = profile.active_model_config()?;
    Some(PromptCacheConfig {
        key: Some(format!("aia:{}:{}:session:{session_id}", profile.name, model.id)),
        retention: Some(RuntimePromptCacheRetention::OneDay),
    })
}

fn broadcast_runtime_events_with_session(
    events: Vec<RuntimeEvent>,
    broadcast_tx: &broadcast::Sender<SsePayload>,
    session_id: &str,
) -> Option<TurnLifecycle> {
    let mut turn = None;
    for event in events {
        match event {
            RuntimeEvent::TurnLifecycle { turn: lifecycle } => turn = Some(lifecycle),
            RuntimeEvent::ContextCompressed { summary } => {
                let _ = broadcast_tx.send(SsePayload::ContextCompressed {
                    session_id: session_id.to_string(),
                    summary,
                });
            }
            _ => {}
        }
    }
    turn
}

fn now_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis().min(u64::MAX as u128) as u64,
        Err(_) => 0,
    }
}

fn update_current_turn_status(
    snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    status: TurnStatus,
) {
    if let Some(current) = write_lock(snapshot).as_mut() {
        current.status = status;
    }
}

fn update_current_turn_from_stream(
    snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    event: &StreamEvent,
) {
    let mut guard = write_lock(snapshot);
    let Some(snap) = guard.as_mut() else { return };

    match event {
        StreamEvent::ThinkingDelta { text } => match snap.blocks.last_mut() {
            Some(CurrentTurnBlock::Thinking { content }) => content.push_str(text),
            _ => snap.blocks.push(CurrentTurnBlock::Thinking { content: text.clone() }),
        },
        StreamEvent::TextDelta { text } => match snap.blocks.last_mut() {
            Some(CurrentTurnBlock::Text { content }) => content.push_str(text),
            _ => snap.blocks.push(CurrentTurnBlock::Text { content: text.clone() }),
        },
        StreamEvent::ToolCallDetected { .. } => {}
        StreamEvent::ToolCallStarted { invocation_id, tool_name, arguments } => {
            if let Some(CurrentTurnBlock::Tool { tool }) =
                snap.blocks.iter_mut().rev().find(|b| {
                    matches!(b, CurrentTurnBlock::Tool { tool } if tool.invocation_id == *invocation_id)
                })
            {
                tool.tool_name = tool_name.clone();
                tool.arguments = object_value(arguments);
                tool.started_at_ms = Some(tool.started_at_ms.unwrap_or_else(now_timestamp_ms));
            } else {
                let ts = now_timestamp_ms();
                snap.blocks.push(CurrentTurnBlock::Tool {
                    tool: CurrentToolOutput {
                        invocation_id: invocation_id.clone(),
                        tool_name: tool_name.clone(),
                        arguments: object_value(arguments),
                        detected_at_ms: ts,
                        started_at_ms: Some(ts),
                        finished_at_ms: None,
                        output: String::new(),
                        completed: false,
                        result_content: None,
                        result_details: None,
                        failed: None,
                    },
                });
            }
        }
        StreamEvent::ToolOutputDelta { invocation_id, text, .. } => {
            if let Some(CurrentTurnBlock::Tool { tool }) =
                snap.blocks.iter_mut().rev().find(|b| {
                    matches!(b, CurrentTurnBlock::Tool { tool } if tool.invocation_id == *invocation_id)
                })
            {
                tool.output.push_str(text);
            } else {
                let ts = now_timestamp_ms();
                snap.blocks.push(CurrentTurnBlock::Tool {
                    tool: CurrentToolOutput {
                        invocation_id: invocation_id.clone(),
                        tool_name: String::new(),
                        arguments: serde_json::json!({}),
                        detected_at_ms: ts,
                        started_at_ms: Some(ts),
                        finished_at_ms: None,
                        output: text.clone(),
                        completed: false,
                        result_content: None,
                        result_details: None,
                        failed: None,
                    },
                });
            }
        }
        StreamEvent::ToolCallCompleted { invocation_id, tool_name, content, details, failed } => {
            if let Some(CurrentTurnBlock::Tool { tool }) =
                snap.blocks.iter_mut().rev().find(|b| {
                    matches!(b, CurrentTurnBlock::Tool { tool } if tool.invocation_id == *invocation_id)
                })
            {
                tool.tool_name = tool_name.clone();
                tool.completed = true;
                tool.finished_at_ms = Some(now_timestamp_ms());
                tool.result_content = Some(content.clone());
                tool.result_details = details.clone();
                tool.failed = Some(*failed);
            }
        }
        StreamEvent::Log { .. } | StreamEvent::Done => {}
    }
}

fn object_value(value: &serde_json::Value) -> serde_json::Value {
    if value.is_object() { value.clone() } else { serde_json::json!({}) }
}

fn persist_tool_trace_spans(turn: &TurnLifecycle, store: &dyn LlmTraceStore) {
    use agent_store::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus};
    use serde_json::json;

    for invocation in &turn.tool_invocations {
        let Some(context) = invocation.trace_context.as_ref() else { continue };

        let failed =
            matches!(&invocation.outcome, agent_runtime::ToolInvocationOutcome::Failed { .. });
        let cancelled = matches!(
            &invocation.outcome,
            agent_runtime::ToolInvocationOutcome::Failed { message } if message.contains("已取消")
        );
        let (status, error, response_summary, response_body, events, details) = match &invocation
            .outcome
        {
            agent_runtime::ToolInvocationOutcome::Succeeded { result } => (
                LlmTraceStatus::Succeeded,
                None,
                json!({"status":"succeeded","tool_name":result.tool_name,"content_preview":preview_text(&result.content)}),
                Some(result.content.clone()),
                vec![
                    LlmTraceEvent {
                        name: "tool.started".into(),
                        at_ms: invocation.started_at_ms,
                        attributes: json!({"invocation_id":invocation.call.invocation_id,"tool_name":invocation.call.tool_name}),
                    },
                    LlmTraceEvent {
                        name: "tool.completed".into(),
                        at_ms: invocation.finished_at_ms,
                        attributes: json!({"invocation_id":result.invocation_id,"tool_name":result.tool_name,"details":result.details}),
                    },
                ],
                result.details.clone(),
            ),
            agent_runtime::ToolInvocationOutcome::Failed { message } => (
                LlmTraceStatus::Failed,
                Some(message.clone()),
                json!({"status":"failed","tool_name":invocation.call.tool_name,"error":message}),
                Some(message.clone()),
                vec![
                    LlmTraceEvent {
                        name: "tool.started".into(),
                        at_ms: invocation.started_at_ms,
                        attributes: json!({"invocation_id":invocation.call.invocation_id,"tool_name":invocation.call.tool_name}),
                    },
                    LlmTraceEvent {
                        name: "tool.failed".into(),
                        at_ms: invocation.finished_at_ms,
                        attributes: json!({"invocation_id":invocation.call.invocation_id,"tool_name":invocation.call.tool_name,"error":message}),
                    },
                ],
                None,
            ),
        };

        let record = LlmTraceRecord {
            id: context.span_id.clone(),
            trace_id: context.trace_id.clone(),
            span_id: context.span_id.clone(),
            parent_span_id: Some(context.parent_span_id.clone()),
            root_span_id: context.root_span_id.clone(),
            operation_name: context.operation_name.clone(),
            span_kind: LlmTraceSpanKind::Internal,
            turn_id: turn.turn_id.clone(),
            run_id: turn.turn_id.clone(),
            request_kind: "tool".into(),
            step_index: context.parent_step_index,
            provider: "runtime".into(),
            protocol: "tool-runtime".into(),
            model: invocation.call.tool_name.clone(),
            base_url: "local://runtime".into(),
            endpoint_path: format!("/tools/{}", invocation.call.tool_name),
            streaming: false,
            started_at_ms: invocation.started_at_ms,
            finished_at_ms: Some(invocation.finished_at_ms),
            duration_ms: Some(invocation.finished_at_ms.saturating_sub(invocation.started_at_ms)),
            status_code: None,
            status,
            stop_reason: None,
            error,
            request_summary: json!({"tool_name":invocation.call.tool_name,"invocation_id":invocation.call.invocation_id,"parent_request_kind":context.parent_request_kind,"parent_step_index":context.parent_step_index}),
            provider_request: json!({"invocation_id":invocation.call.invocation_id,"tool_name":invocation.call.tool_name,"arguments":invocation.call.arguments}),
            response_summary,
            response_body,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cached_tokens: None,
            otel_attributes: json!({"aia.operation.name":context.operation_name,"aia.tool.name":invocation.call.tool_name,"aia.tool.invocation_id":invocation.call.invocation_id,"aia.parent.request_kind":context.parent_request_kind,"aia.parent.step_index":context.parent_step_index,"aia.tool.failed":failed,"aia.tool.cancelled":cancelled,"aia.tool.details":details}),
            events,
        };
        if let Err(e) = store.record(&record) {
            eprintln!("tool trace record failed: {e}");
        }
    }
}

fn preview_text(value: &str) -> String {
    let mut preview = value.chars().take(120).collect::<String>();
    if value.chars().count() > 120 {
        preview.push_str("...");
    }
    preview
}

#[derive(Clone, PartialEq)]
enum CurrentStatusInner {
    Waiting,
    Thinking,
    Working,
    Generating,
}

impl CurrentStatusInner {
    fn to_turn_status(&self) -> TurnStatus {
        match self {
            Self::Waiting => TurnStatus::Waiting,
            Self::Thinking => TurnStatus::Thinking,
            Self::Working => TurnStatus::Working,
            Self::Generating => TurnStatus::Generating,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, RwLock};

    use crate::runtime_worker::RunningTurnHandle;
    use crate::sse::{SsePayload, TurnStatus};

    use super::{
        CurrentTurnSnapshot, handle_cancel_turn, read_lock, update_current_turn_status, write_lock,
    };

    fn poison_lock<T>(lock: &RwLock<T>) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = lock.write().expect("test should acquire write lock before poisoning");
            panic!("poison test lock");
        }));
    }

    #[test]
    fn recovered_read_lock_returns_inner_value_after_poison() {
        let lock = RwLock::new(vec![1, 2, 3]);
        poison_lock(&lock);

        let guard = read_lock(&lock);
        assert_eq!(&*guard, &[1, 2, 3]);
    }

    #[test]
    fn recovered_write_lock_allows_mutation_after_poison() {
        let lock = RwLock::new(vec![1, 2, 3]);
        poison_lock(&lock);

        write_lock(&lock).push(4);
        assert_eq!(&*read_lock(&lock), &[1, 2, 3, 4]);
    }

    #[test]
    fn update_current_turn_status_recovers_from_poisoned_snapshot_lock() {
        let snapshot = Arc::new(RwLock::new(Some(CurrentTurnSnapshot {
            started_at_ms: 1,
            user_message: "hello".into(),
            status: TurnStatus::Waiting,
            blocks: Vec::new(),
        })));
        poison_lock(&snapshot);

        update_current_turn_status(&snapshot, TurnStatus::Generating);

        let guard = read_lock(&snapshot);
        let current = guard.as_ref().expect("snapshot should still exist");
        assert_eq!(current.status, TurnStatus::Generating);
    }

    #[test]
    fn handle_cancel_turn_marks_running_snapshot_as_cancelled() {
        let current_turn = Arc::new(RwLock::new(Some(CurrentTurnSnapshot {
            started_at_ms: 1,
            user_message: "hello".into(),
            status: TurnStatus::Working,
            blocks: Vec::new(),
        })));
        let control = agent_runtime::TurnControl::new(agent_core::AbortSignal::new());
        let handle = RunningTurnHandle { control: control.clone() };
        let mut slots = std::collections::HashMap::new();
        slots.insert(
            "session-1".to_string(),
            super::SessionSlot {
                runtime: None,
                subscriber: 0,
                session_path: std::path::PathBuf::new(),
                history: Arc::new(RwLock::new(Vec::new())),
                current_turn: current_turn.clone(),
                running_turn: Some(handle),
                status: super::SlotStatus::Running,
            },
        );
        let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel(8);
        let config = super::SessionManagerConfig {
            sessions_dir: std::path::PathBuf::new(),
            store: Arc::new(agent_store::AiaStore::new(":memory:").expect("memory store")),
            registry: provider_registry::ProviderRegistry::default(),
            store_path: std::path::PathBuf::new(),
            broadcast_tx,
            provider_registry_snapshot: Arc::new(RwLock::new(
                provider_registry::ProviderRegistry::default(),
            )),
            provider_info_snapshot: Arc::new(RwLock::new(super::ProviderInfoSnapshot {
                name: "bootstrap".into(),
                model: "bootstrap".into(),
                connected: true,
            })),
            workspace_root: std::path::PathBuf::new(),
            user_agent: "test-agent".into(),
        };

        let cancelled =
            handle_cancel_turn(&mut slots, &config, "session-1").expect("cancel succeeds");

        assert!(cancelled);
        assert!(control.abort_signal().is_aborted());
        let guard = read_lock(&current_turn);
        let current = guard.as_ref().expect("snapshot should still exist");
        assert_eq!(current.status, TurnStatus::Cancelled);

        let first_event = broadcast_rx.try_recv().expect("status event should be sent");
        assert!(matches!(first_event, SsePayload::Status { status: TurnStatus::Cancelled, .. }));
        let second_event = broadcast_rx.try_recv().expect("turn_cancelled event should be sent");
        assert!(matches!(second_event, SsePayload::TurnCancelled { .. }));
    }
}
