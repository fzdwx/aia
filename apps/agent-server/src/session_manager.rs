mod current_turn;
mod handle;
#[cfg(test)]
mod tests;
mod tool_trace;
mod types;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use agent_core::{
    ModelIdentity, PromptCacheConfig, PromptCacheRetention as RuntimePromptCacheRetention,
    RequestTimeoutConfig, StreamEvent, ToolRegistry,
};
use agent_runtime::{AgentRuntime, ContextStats, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use agent_store::{AiaStore, SessionRecord, generate_session_id};
use builtin_tools::build_tool_registry;
use provider_registry::ProviderRegistry;
use session_tape::{SessionProviderBinding, SessionTape};
use tokio::sync::{broadcast, mpsc};

use crate::{
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    sse::{SsePayload, TurnStatus},
};
use current_turn::{
    CurrentStatusInner, next_server_turn_id, now_timestamp_ms, refresh_context_stats_snapshot,
    update_current_turn_from_stream, update_current_turn_status,
};
pub use handle::SessionManagerHandle;
use tool_trace::persist_tool_trace_spans;
pub use types::SessionManagerConfig;
use types::{RuntimeReturn, SessionCommand, SessionId, SessionSlot, SlotStatus};

// Re-export types that routes and tests still need
use crate::runtime_worker::rebuild_session_snapshots_from_tape;
pub use crate::runtime_worker::{
    CreateProviderInput, CurrentTurnSnapshot, ProviderInfoSnapshot, RunningTurnHandle,
    RuntimeWorkerError, SwitchProviderInput, UpdateProviderInput,
};

pub fn spawn_session_manager(config: SessionManagerConfig) -> SessionManagerHandle {
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(session_manager_loop(rx, config));
    SessionManagerHandle::new(tx)
}

async fn session_manager_loop(
    mut rx: mpsc::Receiver<SessionCommand>,
    mut config: SessionManagerConfig,
) {
    let mut slots: HashMap<SessionId, SessionSlot> = HashMap::new();
    let (return_tx, mut return_rx) = mpsc::channel::<RuntimeReturn>(64);

    // Load existing sessions from DB and hydrate slots
    if let Ok(records) = config.store.list_sessions_async().await {
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
                        let result = handle_create_session(&mut slots, &config, title).await;
                        let _ = reply.send(result);
                    }
                    SessionCommand::DeleteSession { session_id, reply } => {
                        let result = handle_delete_session(&mut slots, &config, &session_id).await;
                        let _ = reply.send(result);
                    }
                    SessionCommand::SubmitTurn { session_id, prompt, reply } => {
                        let result = handle_submit_turn(&mut slots, &config, &return_tx, &session_id, prompt).await;
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
                    SessionCommand::AutoCompressSession { session_id, reply } => {
                        let result =
                            handle_auto_compress_session(&mut slots, &config, &session_id).await;
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
                    *write_lock(&slot.context_stats) = ret.runtime.context_stats();
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

    let request_timeout = runtime_request_timeout();
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
        .with_max_tool_calls_per_turn(100000)
        .with_request_timeout(request_timeout);
    if let Some(prompt_cache) = prompt_cache {
        runtime = runtime.with_prompt_cache(prompt_cache);
    }

    let subscriber = runtime.subscribe();
    let snapshots = rebuild_session_snapshots_from_tape(runtime.tape());
    let context_stats = runtime.context_stats();

    Ok(SessionSlot {
        runtime: Some(runtime),
        subscriber,
        session_path,
        history: Arc::new(RwLock::new(snapshots.history)),
        current_turn: Arc::new(RwLock::new(snapshots.current_turn)),
        context_stats: Arc::new(RwLock::new(context_stats)),
        running_turn: None,
        status: SlotStatus::Idle,
    })
}

fn runtime_request_timeout() -> RequestTimeoutConfig {
    RequestTimeoutConfig { read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS) }
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

fn collect_runtime_events(
    runtime: &mut AgentRuntime<ServerModel, ToolRegistry>,
    subscriber: RuntimeSubscriberId,
) -> Result<Vec<RuntimeEvent>, RuntimeWorkerError> {
    runtime.collect_events(subscriber).map_err(|error| {
        RuntimeWorkerError::internal(format!("runtime event collection failed: {error}"))
    })
}

async fn handle_create_session(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    title: Option<String>,
) -> Result<SessionRecord, RuntimeWorkerError> {
    let session_id = generate_session_id();
    let title = title.unwrap_or_else(|| aia_config::DEFAULT_SESSION_TITLE.to_string());
    let model_name = read_lock(&config.provider_info_snapshot).model.clone();
    let record = SessionRecord::new(session_id.clone(), title.clone(), model_name);

    config
        .store
        .create_session_async(record.clone())
        .await
        .map_err(|e| RuntimeWorkerError::internal(format!("session db insert failed: {e}")))?;

    let slot = create_slot_for_session(&session_id, config)?;
    slots.insert(session_id.clone(), slot);

    let _ = config
        .broadcast_tx
        .send(SsePayload::SessionCreated { session_id: session_id.clone(), title });

    Ok(record)
}

async fn handle_delete_session(
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
        .delete_session_async(session_id.to_string())
        .await
        .map_err(|e| RuntimeWorkerError::internal(format!("session db delete failed: {e}")))?;

    let _ =
        config.broadcast_tx.send(SsePayload::SessionDeleted { session_id: session_id.to_string() });

    Ok(())
}

async fn handle_submit_turn(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    return_tx: &mpsc::Sender<RuntimeReturn>,
    session_id: &str,
    prompt: String,
) -> Result<String, RuntimeWorkerError> {
    let slot = slots
        .get_mut(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;

    if slot.status == SlotStatus::Running {
        return Err(RuntimeWorkerError::bad_request("a turn is already running in this session"));
    }

    let runtime =
        slot.runtime.take().ok_or_else(|| RuntimeWorkerError::internal("runtime not available"))?;
    *write_lock(&slot.context_stats) = runtime.context_stats();
    let subscriber = slot.subscriber;
    let turn_control = runtime.turn_control();
    slot.running_turn = Some(RunningTurnHandle { control: turn_control.clone() });
    slot.status = SlotStatus::Running;

    let _ = config.store.update_session_async(session_id.to_string(), None, None).await;

    // Initialize current turn snapshot
    let turn_id = next_server_turn_id();
    let current_turn = CurrentTurnSnapshot {
        turn_id: turn_id.clone(),
        started_at_ms: now_timestamp_ms(),
        user_message: prompt.clone(),
        status: TurnStatus::Waiting,
        blocks: Vec::new(),
    };
    *write_lock(&slot.current_turn) = Some(current_turn.clone());

    let broadcast_tx = config.broadcast_tx.clone();
    let current_turn_snapshot = slot.current_turn.clone();
    let history_snapshot = slot.history.clone();
    let context_stats_snapshot = slot.context_stats.clone();
    let trace_store = config.store.clone();
    let sid = session_id.to_string();
    let return_tx = return_tx.clone();
    let turn_control = turn_control.clone();
    let stream_turn_id = turn_id.clone();

    let _ =
        broadcast_tx.send(SsePayload::CurrentTurnStarted { session_id: sid.clone(), current_turn });
    let _ = broadcast_tx.send(SsePayload::Status {
        session_id: sid.clone(),
        turn_id: turn_id.clone(),
        status: TurnStatus::Waiting,
    });

    tokio::spawn(async move {
        let runtime_return = run_turn_worker(
            runtime,
            subscriber,
            prompt,
            turn_control,
            broadcast_tx,
            current_turn_snapshot,
            history_snapshot,
            context_stats_snapshot,
            trace_store,
            sid,
            stream_turn_id,
        )
        .await;

        let _ = return_tx.send(runtime_return).await;
    });

    Ok(turn_id)
}

async fn run_turn_worker(
    mut runtime: AgentRuntime<ServerModel, ToolRegistry>,
    subscriber: RuntimeSubscriberId,
    prompt: String,
    turn_control: agent_runtime::TurnControl,
    broadcast_tx: broadcast::Sender<SsePayload>,
    current_turn_snapshot: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    history_snapshot: Arc<RwLock<Vec<TurnLifecycle>>>,
    context_stats_snapshot: Arc<RwLock<ContextStats>>,
    trace_store: Arc<AiaStore>,
    session_id: SessionId,
    turn_id: String,
) -> RuntimeReturn {
    let mut current_status = CurrentStatusInner::Waiting;
    let status_broadcast = broadcast_tx.clone();
    let stream_session_id = session_id.clone();
    let stream_snapshot = current_turn_snapshot.clone();

    let result = runtime
        .handle_turn_streaming(prompt, turn_control, |event| {
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
                update_current_turn_status(&stream_snapshot, new_status.to_turn_status());
                let _ = status_broadcast.send(SsePayload::Status {
                    session_id: stream_session_id.clone(),
                    turn_id: turn_id.clone(),
                    status: new_status.to_turn_status(),
                });
            }

            update_current_turn_from_stream(&stream_snapshot, &event);
            let _ = status_broadcast.send(SsePayload::Stream {
                session_id: stream_session_id.clone(),
                turn_id: turn_id.clone(),
                event,
            });
        })
        .await;
    *write_lock(&context_stats_snapshot) = runtime.context_stats();

    match result {
        Ok(_) => match collect_runtime_events(&mut runtime, subscriber) {
            Ok(events) => {
                let turn =
                    broadcast_runtime_events_with_session(events, &broadcast_tx, &session_id);
                if let Some(turn) = turn {
                    persist_tool_trace_spans(&turn, trace_store.clone()).await;
                    write_lock(&history_snapshot).push(turn.clone());
                    let _ = broadcast_tx.send(SsePayload::TurnCompleted {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                        turn,
                    });
                }
                *write_lock(&current_turn_snapshot) = None;
            }
            Err(error) => {
                *write_lock(&current_turn_snapshot) = None;
                let _ = broadcast_tx.send(SsePayload::Error {
                    session_id: session_id.clone(),
                    turn_id: Some(turn_id.clone()),
                    message: error.message.clone(),
                });
            }
        },
        Err(error) => {
            let was_cancelled = error.is_cancelled();
            match collect_runtime_events(&mut runtime, subscriber) {
                Ok(events) => {
                    let turn =
                        broadcast_runtime_events_with_session(events, &broadcast_tx, &session_id);
                    if let Some(turn) = turn {
                        persist_tool_trace_spans(&turn, trace_store.clone()).await;
                        write_lock(&history_snapshot).push(turn.clone());
                        let _ = broadcast_tx.send(SsePayload::TurnCompleted {
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            turn,
                        });
                    }
                }
                Err(collection_error) => {
                    let _ = broadcast_tx.send(SsePayload::Error {
                        session_id: session_id.clone(),
                        turn_id: Some(turn_id.clone()),
                        message: collection_error.message.clone(),
                    });
                }
            }
            if was_cancelled {
                update_current_turn_status(&current_turn_snapshot, TurnStatus::Cancelled);
                let _ = broadcast_tx.send(SsePayload::Status {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    status: TurnStatus::Cancelled,
                });
                let _ = broadcast_tx.send(SsePayload::TurnCancelled {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                });
            }
            *write_lock(&current_turn_snapshot) = None;
            let _ = broadcast_tx.send(SsePayload::Error {
                session_id: session_id.clone(),
                turn_id: Some(turn_id.clone()),
                message: error.to_string(),
            });
        }
    }

    RuntimeReturn { session_id, runtime, subscriber }
}

fn handle_cancel_turn(
    slots: &mut HashMap<SessionId, SessionSlot>,
    _config: &SessionManagerConfig,
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

    if let Some(runtime) = slot.runtime.as_ref() {
        return Ok(runtime.context_stats());
    }

    Ok(read_lock(&slot.context_stats).clone())
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
    refresh_context_stats_snapshot(&slot.context_stats, runtime);
    Ok(handoff.anchor.entry_id)
}

async fn handle_auto_compress_session(
    slots: &mut HashMap<SessionId, SessionSlot>,
    config: &SessionManagerConfig,
    session_id: &str,
) -> Result<bool, RuntimeWorkerError> {
    let slot = slots
        .get_mut(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    let runtime = slot
        .runtime
        .as_mut()
        .ok_or_else(|| RuntimeWorkerError::bad_request("session is currently running a turn"))?;

    let compressed = runtime
        .auto_compress_now()
        .await
        .map_err(|error| RuntimeWorkerError::internal(format!("auto compress failed: {error}")))?;
    runtime
        .tape()
        .save_jsonl(&slot.session_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("session save failed: {e}")))?;
    refresh_context_stats_snapshot(&slot.context_stats, runtime);

    if compressed {
        let events = collect_runtime_events(runtime, slot.subscriber)?;
        let _ = broadcast_runtime_events_with_session(events, &config.broadcast_tx, session_id);
    }

    Ok(compressed)
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
            refresh_context_stats_snapshot(&slot.context_stats, runtime);
        }
    }

    config.registry = candidate_registry;
    *write_lock(&config.provider_registry_snapshot) = config.registry.clone();
    *write_lock(&config.provider_info_snapshot) = info.clone();

    Ok(info)
}

fn prepare_runtime_sync(
    registry: &ProviderRegistry,
    trace_store: Option<Arc<AiaStore>>,
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
        key: Some(aia_config::build_prompt_cache_key(&profile.name, &model.id, session_id)),
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

pub(crate) use types::{read_lock, write_lock};
