mod current_turn;
mod handle;
mod prompt;
mod provider_sync;
mod query_ops;
#[cfg(test)]
#[path = "../../tests/session_manager/mod.rs"]
mod tests;
mod tool_trace;
mod turn_execution;
mod types;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use agent_core::{
    ModelIdentity, PromptCacheConfig, PromptCacheRetention as RuntimePromptCacheRetention,
    QuestionRequest, QuestionResult, QuestionResultStatus, ReasoningEffort, ToolCall, ToolResult,
};
use agent_runtime::AgentRuntime;
use agent_store::{AiaStore, SessionRecord, generate_session_id};
use builtin_tools::build_tool_registry;
use provider_registry::ProviderRegistry;
use session_tape::{SessionProviderBinding, SessionTape};
use tokio::sync::mpsc;

use crate::{
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    sse::SsePayload,
};
use current_turn::{
    CurrentStatusInner, next_server_turn_id, now_timestamp_ms, refresh_context_stats_snapshot,
    update_current_turn_from_stream, update_current_turn_status,
};
pub use handle::SessionManagerHandle;
use prompt::build_session_system_prompt;
use provider_sync::{ProviderSyncService, ReturnedRuntimeSync};
pub(crate) use query_ops::SessionQueryService;
use turn_execution::{RuntimeEventProjector, TurnExecutionService, collect_runtime_events};
pub use types::SessionManagerConfig;
#[cfg(test)]
pub(crate) use types::SlotExecutionState;
use types::{RuntimeReturn, SessionCommand, SessionId, SessionSlot, SlotStatus};

#[cfg(test)]
pub(crate) use crate::runtime_worker::CurrentTurnSnapshot;
use crate::runtime_worker::rebuild_session_snapshots_from_tape;
pub use crate::runtime_worker::{
    CreateProviderInput, ProviderInfoSnapshot, RuntimeWorkerError, SwitchProviderInput,
    UpdateProviderInput,
};
pub(crate) use tool_trace::ToolTraceRecorder;

pub fn spawn_session_manager(config: SessionManagerConfig) -> SessionManagerHandle {
    let workspace_root = config.workspace_root.clone();
    let (command_tx, command_rx) = mpsc::channel(256);
    let (return_tx, return_rx) = mpsc::channel(64);
    tokio::spawn(
        SessionManagerLoop::new(config, command_tx.clone(), command_rx, return_tx, return_rx).run(),
    );
    SessionManagerHandle::new(command_tx, workspace_root)
}

const UNAVAILABLE_SESSION_MODEL: &str = "unavailable";

struct SessionManagerLoop {
    slots: HashMap<SessionId, SessionSlot>,
    hydration_errors: HashMap<SessionId, RuntimeWorkerError>,
    config: SessionManagerConfig,
    command_tx: mpsc::Sender<SessionCommand>,
    command_rx: mpsc::Receiver<SessionCommand>,
    return_tx: mpsc::Sender<RuntimeReturn>,
    return_rx: mpsc::Receiver<RuntimeReturn>,
}

impl SessionManagerLoop {
    fn new(
        config: SessionManagerConfig,
        command_tx: mpsc::Sender<SessionCommand>,
        command_rx: mpsc::Receiver<SessionCommand>,
        return_tx: mpsc::Sender<RuntimeReturn>,
        return_rx: mpsc::Receiver<RuntimeReturn>,
    ) -> Self {
        Self {
            slots: HashMap::new(),
            hydration_errors: HashMap::new(),
            config,
            command_tx,
            command_rx,
            return_tx,
            return_rx,
        }
    }

    async fn run(mut self) {
        self.hydrate_slots().await;

        loop {
            tokio::select! {
                Some(command) = self.command_rx.recv() => self.handle_command(command).await,
                Some(ret) = self.return_rx.recv() => self.handle_runtime_return(ret),
                else => break,
            }
        }
    }

    async fn hydrate_slots(&mut self) {
        if let Ok(records) = self.config.store.list_sessions_async().await {
            let slot_factory = SessionSlotFactory::new(&self.config);
            for record in records {
                match slot_factory.create(&record.id) {
                    Ok(slot) => {
                        self.hydration_errors.remove(&record.id);
                        self.slots.insert(record.id, slot);
                    }
                    Err(error) => {
                        self.hydration_errors.insert(
                            record.id,
                            RuntimeWorkerError::internal(format!(
                                "session recovery failed: {}",
                                error.message
                            )),
                        );
                    }
                }
            }
        }
    }

    async fn handle_command(&mut self, command: SessionCommand) {
        match command {
            SessionCommand::ListSessions { reply } => {
                let _ = reply.send(self.list_sessions().await);
            }
            SessionCommand::CreateSession { title, reply } => {
                let _ = reply.send(self.create_session(title).await);
            }
            SessionCommand::DeleteSession { session_id, reply } => {
                let _ = reply.send(self.delete_session(&session_id).await);
            }
            SessionCommand::SubmitTurn { session_id, prompt, reply } => {
                let mut turn_execution =
                    TurnExecutionService::new(&mut self.slots, &self.config, &self.return_tx);
                let _ = reply.send(turn_execution.submit_turn(&session_id, prompt).await);
            }
            SessionCommand::CancelTurn { session_id, reply } => {
                let mut query = SessionQueryService::new(&mut self.slots);
                let _ = reply.send(query.cancel_turn(&session_id));
            }
            SessionCommand::GetHistory { session_id, reply } => {
                let query = SessionQueryService::new(&mut self.slots);
                let _ = reply.send(query.history(&session_id));
            }
            SessionCommand::GetCurrentTurn { session_id, reply } => {
                let query = SessionQueryService::new(&mut self.slots);
                let _ = reply.send(query.current_turn(&session_id));
            }
            SessionCommand::GetSessionInfo { session_id, reply } => {
                let query = SessionQueryService::new(&mut self.slots);
                let _ = reply.send(query.session_info(&session_id));
            }
            SessionCommand::CreateHandoff { session_id, name, summary, reply } => {
                let _ = reply.send(self.create_handoff(&session_id, name, summary));
            }
            SessionCommand::AutoCompressSession { session_id, reply } => {
                let _ = reply.send(self.auto_compress_session(&session_id).await);
            }
            SessionCommand::GetSessionSettings { session_id, reply } => {
                let _ = reply.send(self.get_session_settings(&session_id));
            }
            SessionCommand::GetPendingQuestion { session_id, reply } => {
                let _ = reply.send(self.get_pending_question(&session_id));
            }
            SessionCommand::ResolvePendingQuestion { session_id, result, reply } => {
                let _ = reply.send(self.resolve_pending_question(&session_id, result));
            }
            SessionCommand::CancelPendingQuestion { session_id, reply } => {
                let _ = reply.send(self.cancel_pending_question(&session_id));
            }
            SessionCommand::UpdateSessionSettings { session_id, provider_binding, reply } => {
                let mut provider_sync = ProviderSyncService::new(&mut self.slots, &mut self.config);
                let result =
                    provider_sync.update_session_provider_binding(&session_id, provider_binding);
                let _ = reply.send(result);
            }
            SessionCommand::CreateProvider { input, reply } => {
                let mut provider_sync = ProviderSyncService::new(&mut self.slots, &mut self.config);
                let _ = reply.send(provider_sync.create_provider(input));
            }
            SessionCommand::UpdateProvider { name, input, reply } => {
                let mut provider_sync = ProviderSyncService::new(&mut self.slots, &mut self.config);
                let _ = reply.send(provider_sync.update_provider(name, input));
            }
            SessionCommand::DeleteProvider { name, reply } => {
                let mut provider_sync = ProviderSyncService::new(&mut self.slots, &mut self.config);
                let _ = reply.send(provider_sync.delete_provider(name));
            }
            SessionCommand::SwitchProvider { input, reply } => {
                let mut provider_sync = ProviderSyncService::new(&mut self.slots, &mut self.config);
                let _ = reply.send(provider_sync.switch_provider(input));
            }
        }
    }

    fn handle_runtime_return(&mut self, mut ret: RuntimeReturn) {
        if let Some(slot) = self.slots.get_mut(&ret.session_id) {
            let session_path = slot.session_path.clone();
            let pending_provider_binding = slot.take_pending_provider_binding();
            let sync = ReturnedRuntimeSync::new(
                &ret.session_id,
                &session_path,
                &self.config.registry,
                self.config.store.clone(),
                pending_provider_binding,
            );
            if let Err(error) = sync.apply(&mut ret.runtime) {
                let _ = self.config.broadcast_tx.send(SsePayload::Error {
                    session_id: ret.session_id.clone(),
                    turn_id: None,
                    message: error.message,
                });
            }

            *write_lock(&slot.context_stats) = ret.runtime.context_stats();
            slot.provider_binding = ret
                .runtime
                .tape()
                .latest_provider_binding()
                .unwrap_or(SessionProviderBinding::Bootstrap);
            let should_auto_compress = slot
                .context_stats
                .read()
                .ok()
                .and_then(|stats| stats.pressure_ratio)
                .is_some_and(|ratio| ratio >= agent_prompts::AUTO_COMPRESSION_THRESHOLD);
            if let Err(error) = slot.finish_turn(ret.runtime, ret.subscriber) {
                let _ = self.config.broadcast_tx.send(SsePayload::Error {
                    session_id: ret.session_id.clone(),
                    turn_id: None,
                    message: error.message,
                });
                return;
            }

            if should_auto_compress {
                let (reply, _reply_rx) = tokio::sync::oneshot::channel();
                let _ = self.command_tx.try_send(SessionCommand::AutoCompressSession {
                    session_id: ret.session_id,
                    reply,
                });
            }
        }
    }

    async fn create_session(
        &mut self,
        title: Option<String>,
    ) -> Result<SessionRecord, RuntimeWorkerError> {
        let session_id = generate_session_id();
        let title = title.unwrap_or_else(|| aia_config::DEFAULT_SESSION_TITLE.to_string());
        let model_name = read_lock(&self.config.provider_info_snapshot).model.clone();
        let record = SessionRecord::new(session_id.clone(), title.clone(), model_name);

        self.config.store.create_session_async(record.clone()).await.map_err(|error| {
            RuntimeWorkerError::internal(format!("session db insert failed: {error}"))
        })?;

        let slot = SessionSlotFactory::new(&self.config).create(&session_id)?;
        self.hydration_errors.remove(&session_id);
        self.slots.insert(session_id.clone(), slot);

        let _ = self
            .config
            .broadcast_tx
            .send(SsePayload::SessionCreated { session_id: session_id.clone(), title });

        Ok(record)
    }

    async fn list_sessions(&mut self) -> Result<Vec<SessionRecord>, RuntimeWorkerError> {
        let mut records = self.config.store.list_sessions_async().await.map_err(|error| {
            RuntimeWorkerError::internal(format!("session list failed: {error}"))
        })?;

        for record in &mut records {
            record.model = match self.slots.get(&record.id) {
                Some(slot) => projected_session_model(&self.config.registry, slot)?,
                None => UNAVAILABLE_SESSION_MODEL.to_string(),
            };
        }

        Ok(records)
    }

    async fn delete_session(&mut self, session_id: &str) -> Result<(), RuntimeWorkerError> {
        if let Some(slot) = self.slots.get(session_id)
            && slot.status() == SlotStatus::Running
        {
            return Err(RuntimeWorkerError::bad_request(
                "cannot delete a session while a turn is running",
            ));
        }

        self.slots.remove(session_id);
        self.hydration_errors.remove(session_id);

        let session_path = self.config.sessions_dir.join(format!("{session_id}.jsonl"));
        if session_path.exists() {
            std::fs::remove_file(&session_path).map_err(|error| {
                RuntimeWorkerError::internal(format!("jsonl delete failed: {error}"))
            })?;
        }

        self.config.store.delete_session_async(session_id.to_string()).await.map_err(|error| {
            RuntimeWorkerError::internal(format!("session db delete failed: {error}"))
        })?;
        self.config
            .store
            .delete_channel_bindings_by_session_id_async(session_id.to_string())
            .await
            .map_err(|error| {
                RuntimeWorkerError::internal(format!("channel binding delete failed: {error}"))
            })?;

        let _ = self
            .config
            .broadcast_tx
            .send(SsePayload::SessionDeleted { session_id: session_id.to_string() });

        Ok(())
    }

    fn create_handoff(
        &mut self,
        session_id: &str,
        name: String,
        summary: String,
    ) -> Result<u64, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;
        let session_path = slot.session_path.clone();
        let context_stats = slot.context_stats.clone();
        let runtime = slot.runtime_mut().ok_or_else(|| {
            RuntimeWorkerError::bad_request("session is currently running a turn")
        })?;

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
        runtime.tape().save_jsonl(&session_path).map_err(|error| {
            RuntimeWorkerError::internal(format!("session save failed: {error}"))
        })?;
        refresh_context_stats_snapshot(&context_stats, runtime);
        Ok(handoff.anchor.entry_id)
    }

    async fn auto_compress_session(
        &mut self,
        session_id: &str,
    ) -> Result<bool, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;
        let session_path = slot.session_path.clone();
        let context_stats = slot.context_stats.clone();
        let subscriber = slot.subscriber();
        let runtime = slot.runtime_mut().ok_or_else(|| {
            RuntimeWorkerError::bad_request("session is currently running a turn")
        })?;

        let compressed = runtime.auto_compress_now().await.map_err(|error| {
            RuntimeWorkerError::internal(format!("auto compress failed: {error}"))
        })?;
        runtime.tape().save_jsonl(&session_path).map_err(|error| {
            RuntimeWorkerError::internal(format!("session save failed: {error}"))
        })?;
        refresh_context_stats_snapshot(&context_stats, runtime);

        if compressed {
            let events = collect_runtime_events(runtime, subscriber)?;
            let _ =
                RuntimeEventProjector::new(&self.config.broadcast_tx, session_id).project(events);
        }

        Ok(compressed)
    }

    fn get_session_settings(
        &mut self,
        session_id: &str,
    ) -> Result<SessionProviderBinding, RuntimeWorkerError> {
        let slot = self.slots.get(session_id).ok_or_else(|| {
            self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })
        })?;

        if slot.runtime().is_some() || slot.status() == SlotStatus::Running {
            return Ok(slot.provider_binding.clone());
        }

        let tape = SessionTape::load_jsonl_or_default(&slot.session_path)
            .map_err(|error| RuntimeWorkerError::internal(format!("tape load failed: {error}")))?;
        tape.try_latest_provider_binding()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))
            .map(|binding| binding.unwrap_or(SessionProviderBinding::Bootstrap))
    }

    fn get_pending_question(
        &mut self,
        session_id: &str,
    ) -> Result<Option<QuestionRequest>, RuntimeWorkerError> {
        let slot = self.slots.get(session_id).ok_or_else(|| {
            self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })
        })?;

        let tape = SessionTape::load_jsonl_or_default(&slot.session_path)
            .map_err(|error| RuntimeWorkerError::internal(format!("tape load failed: {error}")))?;

        tape.try_pending_question_request()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))
    }

    fn resolve_pending_question(
        &mut self,
        session_id: &str,
        result: QuestionResult,
    ) -> Result<(), RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })
        })?;

        let mut tape = SessionTape::load_jsonl_or_default(&slot.session_path)
            .map_err(|error| RuntimeWorkerError::internal(format!("tape load failed: {error}")))?;
        let pending_request = tape
            .try_pending_question_request()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))?
            .ok_or_else(|| RuntimeWorkerError::bad_request("session has no pending question"))?;

        if pending_request.request_id != result.request_id {
            return Err(RuntimeWorkerError::bad_request(
                "question request_id does not match current pending question",
            ));
        }

        tape.record_question_resolved(&result);
        let call = ToolCall::new("Question")
            .with_invocation_id(pending_request.invocation_id.clone())
            .with_arguments_value(serde_json::json!({
                "questions": pending_request.questions,
            }));
        let details = serde_json::to_value(&result).map_err(|error| {
            RuntimeWorkerError::internal(format!("question result serialization failed: {error}"))
        })?;
        let content = serde_json::to_string(&result).map_err(|error| {
            RuntimeWorkerError::internal(format!("question result encoding failed: {error}"))
        })?;
        tape.append_entry(
            session_tape::TapeEntry::tool_result(
                &ToolResult::from_call(&call, content).with_details(details),
            )
            .with_run_id(&pending_request.turn_id),
        );
        tape.save_jsonl(&slot.session_path).map_err(|error| {
            RuntimeWorkerError::internal(format!("session save failed: {error}"))
        })?;

        let existing_stats = read_lock(&slot.context_stats).clone();
        if let Some(runtime) = slot.runtime_mut() {
            *runtime.tape_mut() = tape.clone();
        }
        let snapshots = rebuild_session_snapshots_from_tape(&tape);
        *write_lock(&slot.history) = snapshots.history;
        *write_lock(&slot.current_turn) = snapshots.current_turn;
        *write_lock(&slot.context_stats) =
            slot.runtime().map(|runtime| runtime.context_stats()).unwrap_or(existing_stats);

        Ok(())
    }

    fn cancel_pending_question(&mut self, session_id: &str) -> Result<(), RuntimeWorkerError> {
        let pending_request = self
            .get_pending_question(session_id)?
            .ok_or_else(|| RuntimeWorkerError::bad_request("session has no pending question"))?;
        self.resolve_pending_question(
            session_id,
            QuestionResult {
                status: QuestionResultStatus::Cancelled,
                request_id: pending_request.request_id,
                answers: Vec::new(),
                reason: None,
            },
        )
    }
}

struct SessionSlotFactory<'a> {
    config: &'a SessionManagerConfig,
}

impl<'a> SessionSlotFactory<'a> {
    fn new(config: &'a SessionManagerConfig) -> Self {
        Self { config }
    }

    fn create(&self, session_id: &str) -> Result<SessionSlot, RuntimeWorkerError> {
        let session_path = self.config.sessions_dir.join(format!("{session_id}.jsonl"));
        let tape = SessionTape::load_jsonl_or_default(&session_path)
            .map_err(|error| RuntimeWorkerError::internal(format!("tape load failed: {error}")))?;
        let provider_binding = tape
            .try_latest_provider_binding()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))?
            .unwrap_or(SessionProviderBinding::Bootstrap);

        let selection = choose_provider_for_tape(&self.config.registry, &tape);
        let prompt_cache = prompt_cache_for_selection(&selection, session_id);
        let (identity, model) =
            build_model_from_selection(selection, Some(self.config.store.clone())).map_err(
                |error: crate::model::ServerSetupError| {
                    RuntimeWorkerError::internal(error.to_string())
                },
            )?;

        let session_append_path = session_path.clone();
        let workspace_root = self.config.workspace_root.clone();
        let mut runtime = AgentRuntime::with_tape(model, build_tool_registry(), identity, tape)
            .with_instructions(build_session_system_prompt(
                self.config.system_prompt.as_deref(),
                &self.config.workspace_root,
            ))
            .with_hooks(self.config.runtime_hooks.clone())
            .with_session_id(session_id.to_string())
            .with_user_agent(self.config.user_agent.clone())
            .with_workspace_root(workspace_root)
            .with_tape_entry_listener(move |entry| {
                SessionTape::append_jsonl_entry(&session_append_path, entry)
            })
            .with_max_tool_calls_per_turn(100000)
            .with_request_timeout(self.config.request_timeout.clone());
        if let Some(prompt_cache) = prompt_cache {
            runtime = runtime.with_prompt_cache(prompt_cache);
        }

        let subscriber = runtime.subscribe();
        let snapshots = rebuild_session_snapshots_from_tape(runtime.tape());
        let context_stats = runtime.context_stats();

        Ok(SessionSlot::idle(
            session_path,
            provider_binding,
            Arc::new(RwLock::new(snapshots.history)),
            Arc::new(RwLock::new(snapshots.current_turn)),
            Arc::new(RwLock::new(context_stats)),
            runtime,
            subscriber,
        ))
    }
}

fn choose_provider_for_tape(
    registry: &ProviderRegistry,
    tape: &SessionTape,
) -> ProviderLaunchChoice {
    if let Some(binding) = tape.latest_provider_binding() {
        match binding {
            SessionProviderBinding::Bootstrap => return ProviderLaunchChoice::Bootstrap,
            SessionProviderBinding::Provider {
                name,
                model,
                base_url,
                protocol,
                reasoning_effort,
            } => {
                if let Some(profile) = registry.providers().iter().find(|provider| {
                    provider.name == name
                        && provider.has_model(&model)
                        && provider.base_url == base_url
                        && provider.kind.protocol_name() == protocol.as_str()
                }) {
                    return ProviderLaunchChoice::OpenAi {
                        profile: profile.clone(),
                        model,
                        reasoning_effort: ReasoningEffort::parse_persisted(reasoning_effort),
                    };
                }
            }
        }
    }

    registry
        .active_provider()
        .cloned()
        .map(|profile| ProviderLaunchChoice::OpenAi {
            model: profile.default_model_id().unwrap_or("").to_string(),
            profile,
            reasoning_effort: None,
        })
        .unwrap_or(ProviderLaunchChoice::Bootstrap)
}

fn projected_session_model(
    registry: &ProviderRegistry,
    slot: &SessionSlot,
) -> Result<String, RuntimeWorkerError> {
    if let Some(runtime) = slot.runtime() {
        return Ok(runtime.model_identity().name.clone());
    }

    if slot.status() == SlotStatus::Running {
        return Ok(match &slot.provider_binding {
            SessionProviderBinding::Bootstrap => "bootstrap".into(),
            SessionProviderBinding::Provider { model, .. } => model.clone(),
        });
    }

    let tape = SessionTape::load_jsonl_or_default(&slot.session_path)
        .map_err(|error| RuntimeWorkerError::internal(format!("tape load failed: {error}")))?;
    let selection = choose_provider_for_tape(registry, &tape);
    Ok(crate::model::model_identity_from_selection(&selection).name)
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
        .map(|profile| ProviderLaunchChoice::OpenAi {
            model: profile.default_model_id().unwrap_or("").to_string(),
            profile,
            reasoning_effort: None,
        })
        .unwrap_or(ProviderLaunchChoice::Bootstrap);

    let (identity, model) = build_model_from_selection(selection, trace_store).map_err(
        |error: crate::model::ServerSetupError| RuntimeWorkerError::internal(error.to_string()),
    )?;

    let binding = match registry.active_provider() {
        Some(profile) => SessionProviderBinding::Provider {
            name: profile.name.clone(),
            model: profile.default_model_id().unwrap_or("").to_string(),
            base_url: profile.base_url.clone(),
            protocol: profile.kind.protocol_name().to_string(),
            reasoning_effort: None,
        },
        None => SessionProviderBinding::Bootstrap,
    };

    let info = ProviderInfoSnapshot::from_identity(&identity);
    Ok((info, identity, model, binding))
}

fn prompt_cache_for_selection(
    selection: &ProviderLaunchChoice,
    session_id: &str,
) -> Option<PromptCacheConfig> {
    let ProviderLaunchChoice::OpenAi { profile, model, .. } = selection else {
        return None;
    };
    let model = profile.models.iter().find(|candidate| candidate.id == *model)?;
    Some(PromptCacheConfig {
        key: Some(aia_config::build_prompt_cache_key(&profile.name, &model.id, session_id)),
        retention: Some(RuntimePromptCacheRetention::OneDay),
    })
}

pub(crate) use types::{read_lock, write_lock};
