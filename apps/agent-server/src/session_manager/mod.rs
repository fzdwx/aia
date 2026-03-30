mod auto_rename;
mod current_turn;
mod handle;
mod message_queue;
mod prompt;
mod provider_sync;
mod query_ops;
mod server_runtime_tool_host;
#[cfg(test)]
#[path = "../../tests/session_manager/mod.rs"]
mod tests;
mod tool_trace;
mod turn_execution;
mod types;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use agent_core::{
    ModelIdentity, PromptCacheConfig, PromptCacheRetention as RuntimePromptCacheRetention,
    QuestionRequest, QuestionResult, QuestionResultStatus, ReasoningEffort,
};
use agent_runtime::AgentRuntime;
use agent_store::{
    AiaStore, SessionAutoRenamePolicy, SessionRecord, SessionTitleSource, generate_session_id,
};
use provider_registry::ProviderRegistry;
use session_tape::{SessionProviderBinding, SessionTape, TapeEntry};
use tokio::sync::mpsc;

use crate::{
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    sse::SsePayload,
};
use auto_rename::SessionAutoRenameService;
use current_turn::{
    CurrentStatusInner, next_server_turn_id, now_timestamp_ms, refresh_context_stats_snapshot,
    update_current_turn_from_stream, update_current_turn_status,
};
pub use handle::SessionManagerHandle;
use message_queue::{MAX_QUEUE_SIZE, QueuedMessage, generate_message_id};
pub use message_queue::{QueueMessageResponse, QueueMessageStatus};
use prompt::build_session_system_prompt;
use provider_sync::{ProviderSyncService, ReturnedRuntimeSync};
pub(crate) use query_ops::SessionQueryService;
use server_runtime_tool_host::ServerRuntimeToolHost;
use turn_execution::{RuntimeEventProjector, TurnExecutionService, collect_runtime_events};
#[cfg(test)]
pub(crate) use types::SlotExecutionState;
use types::{RuntimeReturn, SessionCommand, SessionId, SessionSlot, SlotStatus};
pub use types::{RuntimeToolHost, SessionManagerConfig};

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
    let config = SessionManagerConfig {
        runtime_tool_host: Arc::new(types::RuntimeToolHost { tx: command_tx.clone() }),
        ..config
    };
    tokio::spawn(
        SessionManagerLoop::new(config, command_tx.clone(), command_rx, return_tx, return_rx).run(),
    );
    SessionManagerHandle::new(command_tx, workspace_root)
}

const UNAVAILABLE_SESSION_MODEL: &str = "unavailable";

fn load_session_tape_with_repair(session_path: &Path) -> Result<SessionTape, RuntimeWorkerError> {
    if !session_path.exists() {
        return Ok(SessionTape::new());
    }

    let contents = std::fs::read_to_string(session_path)
        .map_err(|error| RuntimeWorkerError::internal(format!("tape load failed: {error}")))?;
    let mut tape = SessionTape::new();
    let mut repaired = false;

    for (index, line) in contents.lines().filter(|line| !line.trim().is_empty()).enumerate() {
        let mut entry: TapeEntry = serde_json::from_str(line).map_err(|error| {
            RuntimeWorkerError::internal(format!(
                "tape decode failed at line {}: {error}",
                index + 1
            ))
        })?;
        let expected_id = (index as u64) + 1;
        if entry.id != expected_id {
            repaired = true;
        }
        entry.id = 0;
        tape.append_entry(entry);
    }

    if repaired {
        tape.save_jsonl(session_path).map_err(|error| {
            RuntimeWorkerError::internal(format!("session save failed: {error}"))
        })?;
    }

    Ok(tape)
}

fn refresh_runtime_tape_from_disk(
    session_path: &Path,
    runtime: &mut AgentRuntime<ServerModel, agent_core::ToolRegistry>,
) -> Result<(), RuntimeWorkerError> {
    *runtime.tape_mut() = load_session_tape_with_repair(session_path)?;
    Ok(())
}

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
            SessionCommand::CreateSession { title, title_source, auto_rename_policy, reply } => {
                let _ =
                    reply.send(self.create_session(title, title_source, auto_rename_policy).await);
            }
            SessionCommand::DeleteSession { session_id, reply } => {
                let _ = reply.send(self.delete_session(&session_id).await);
            }
            SessionCommand::SubmitTurn { session_id, prompts, reply } => {
                let mut turn_execution =
                    TurnExecutionService::new(&mut self.slots, &self.config, &self.return_tx);
                let _ = reply.send(turn_execution.submit_turn(&session_id, prompts).await);
            }
            SessionCommand::CancelTurn { session_id, reply } => {
                let mut query = SessionQueryService::new(&mut self.slots, &self.hydration_errors);
                let result = query.cancel_turn(&session_id);
                drop(query);
                if matches!(result, Ok(true)) {
                    match self.cancel_pending_question(&session_id) {
                        Ok(()) => {}
                        Err(error) if error.message == "session has no pending question" => {}
                        Err(error) => {
                            let _ = reply.send(Err(error));
                            return;
                        }
                    }
                }
                let _ = reply.send(result);
            }
            SessionCommand::GetHistory { session_id, reply } => {
                let query = SessionQueryService::new(&mut self.slots, &self.hydration_errors);
                let _ = reply.send(query.history(&session_id));
            }
            SessionCommand::GetCurrentTurn { session_id, reply } => {
                let query = SessionQueryService::new(&mut self.slots, &self.hydration_errors);
                let _ = reply.send(query.current_turn(&session_id));
            }
            SessionCommand::GetSessionInfo { session_id, reply } => {
                let query = SessionQueryService::new(&mut self.slots, &self.hydration_errors);
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
            SessionCommand::AskQuestion { session_id, request, reply } => {
                let result = self.register_pending_question(&session_id, request);
                match result {
                    Ok(receiver) => {
                        tokio::spawn(async move {
                            let outcome = receiver.await.map_err(|_| {
                                RuntimeWorkerError::internal("question waiter dropped")
                            });
                            let _ = reply.send(outcome);
                        });
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
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
            SessionCommand::QueueMessage { session_id, content, reply } => {
                let _ = reply.send(self.handle_queue_message(&session_id, content).await);
            }
            SessionCommand::GetQueue { session_id, reply } => {
                let _ = reply.send(self.handle_get_queue(&session_id));
            }
            SessionCommand::DeleteQueuedMessage { session_id, message_id, reply } => {
                let _ = reply.send(self.handle_delete_queued_message(&session_id, &message_id));
            }
            SessionCommand::InterruptTurn { session_id, reply } => {
                let _ = reply.send(self.handle_interrupt_turn(&session_id));
            }
            SessionCommand::SubmitQueuedMessages { session_id, messages, reply } => {
                let mut turn_execution =
                    TurnExecutionService::new(&mut self.slots, &self.config, &self.return_tx);
                let _ = reply.send(turn_execution.submit_turn(&session_id, messages).await);
            }
        }
    }

    fn handle_runtime_return(&mut self, mut ret: RuntimeReturn) {
        // 先提取队列数据（如果有的话），避免多次借用
        let queued_messages = {
            if let Some(slot) = self.slots.get_mut(&ret.session_id) {
                let session_path = slot.session_path.clone();
                if let Err(error) = refresh_runtime_tape_from_disk(&session_path, &mut ret.runtime) {
                    let _ = self.config.broadcast_tx.send(SsePayload::Error {
                        session_id: ret.session_id.clone(),
                        turn_id: None,
                        message: error.message,
                    });
                }
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

                // 保存中断标志并重置（在 finish_turn 之前）
                slot.interrupt_requested = false;

                if let Err(error) = slot.finish_turn(ret.runtime, ret.subscriber) {
                    let _ = self.config.broadcast_tx.send(SsePayload::Error {
                        session_id: ret.session_id.clone(),
                        turn_id: None,
                        message: error.message,
                    });
                    return;
                }

                // 收集队列消息（打断后仍然处理队列）
                let queue_messages = if !slot.message_queue.is_empty() {
                    // 设置队列处理标志，防止新消息直接开始 turn
                    slot.queue_processing = true;

                    let messages: Vec<QueuedMessage> = slot.message_queue.drain(..).collect();

                    // 通过 runtime 追加 dequeued 事件到 tape（会有正确的 ID 分配）
                    if let Some(runtime) = slot.runtime_mut() {
                        for msg in &messages {
                            let entry = TapeEntry::event("message_dequeued", Some(serde_json::json!({
                                "id": msg.id
                            })));
                            let _ = runtime.append_tape_entry(entry);
                        }
                    }

                    Some(messages.iter().map(|m| m.content.clone()).collect::<Vec<String>>())
                } else {
                    None
                };

                if should_auto_compress {
                    let (reply, _reply_rx) = tokio::sync::oneshot::channel();
                    let _ = self.command_tx.try_send(SessionCommand::AutoCompressSession {
                        session_id: ret.session_id.clone(),
                        reply,
                    });
                }

                queue_messages
            } else {
                None
            }
        };

        // 处理队列消息
        if let Some(contents) = queued_messages {
            // 广播队列处理事件
            let _ = self.config.broadcast_tx.send(SsePayload::QueueProcessing {
                session_id: ret.session_id.clone(),
                count: contents.len() as u32,
            });

            // 通过 command_tx 发送新命令来开始 turn
            let _ = self.command_tx.try_send(SessionCommand::SubmitQueuedMessages {
                session_id: ret.session_id,
                messages: contents,
                reply: drop_reply_channel(),
            });
        }
    }

    async fn create_session(
        &mut self,
        title: Option<String>,
        title_source: Option<SessionTitleSource>,
        auto_rename_policy: Option<SessionAutoRenamePolicy>,
    ) -> Result<SessionRecord, RuntimeWorkerError> {
        let session_id = generate_session_id();
        let title = title.unwrap_or_else(|| aia_config::DEFAULT_SESSION_TITLE.to_string());
        let model_name = read_lock(&self.config.provider_info_snapshot).model.clone();
        let (title_source, auto_rename_policy) =
            if let (Some(title_source), Some(auto_rename_policy)) =
                (title_source, auto_rename_policy)
            {
                (title_source, auto_rename_policy)
            } else if title == aia_config::DEFAULT_SESSION_TITLE {
                (SessionTitleSource::Default, SessionAutoRenamePolicy::Enabled)
            } else {
                (SessionTitleSource::Manual, SessionAutoRenamePolicy::Enabled)
            };
        let record = SessionRecord::new_with_metadata(
            session_id.clone(),
            title.clone(),
            model_name,
            title_source,
            auto_rename_policy,
        );

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

        let tape = load_session_tape_with_repair(&slot.session_path)?;
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

        let tape = load_session_tape_with_repair(&slot.session_path)?;

        tape.try_pending_question_request()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))
    }

    fn register_pending_question(
        &mut self,
        session_id: &str,
        request: QuestionRequest,
    ) -> Result<tokio::sync::oneshot::Receiver<QuestionResult>, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })
        })?;

        // 注意：这里加载了新的 tape，可能与 TurnWorker 的写入冲突
        // TODO: 应该通过 runtime 的 tape 来写入，但 runtime 在 Running 状态下被 TurnWorker 持有
        // 需要 refactor 让 pending question 的写入也通过 runtime 进行
        let mut tape = load_session_tape_with_repair(&slot.session_path)?;
        if tape
            .try_pending_question_request()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))?
            .is_some()
            || !slot.pending_question_waiters.is_empty()
        {
            return Err(RuntimeWorkerError::bad_request("session already has a pending question"));
        }
        tape.record_question_requested(&request);
        let entry = tape.entries().last().cloned().ok_or_else(|| {
            RuntimeWorkerError::internal("question request was not appended to tape")
        })?;
        SessionTape::append_jsonl_entry(&slot.session_path, &entry).map_err(|error| {
            RuntimeWorkerError::internal(format!("session append failed: {error}"))
        })?;

        let (sender, receiver) = tokio::sync::oneshot::channel();
        slot.insert_pending_question_waiter(request.request_id.clone(), sender);

        *write_lock(&slot.current_turn) = Some(crate::runtime_worker::CurrentTurnSnapshot {
            turn_id: request.turn_id.clone(),
            started_at_ms: now_timestamp_ms(),
            user_messages: slot
                .current_turn
                .read()
                .ok()
                .and_then(|current| current.as_ref().map(|turn| turn.user_messages.clone()))
                .unwrap_or_default(),
            status: crate::sse::TurnStatus::WaitingForQuestion,
            blocks: slot
                .current_turn
                .read()
                .ok()
                .and_then(|current| current.as_ref().map(|turn| turn.blocks.clone()))
                .unwrap_or_default(),
        });
        let _ = self.config.broadcast_tx.send(SsePayload::Status {
            session_id: session_id.to_string(),
            turn_id: request.turn_id.clone(),
            status: crate::sse::TurnStatus::WaitingForQuestion,
        });

        Ok(receiver)
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

        // 注意：这里加载了新的 tape，可能与 TurnWorker 的写入冲突
        // TODO: 应该通过 runtime 的 tape 来写入，但 runtime 在 Running 状态下被 TurnWorker 持有
        let mut tape = load_session_tape_with_repair(&slot.session_path)?;
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
        let entry = tape.entries().last().cloned().ok_or_else(|| {
            RuntimeWorkerError::internal("question result was not appended to tape")
        })?;
        SessionTape::append_jsonl_entry(&slot.session_path, &entry).map_err(|error| {
            RuntimeWorkerError::internal(format!("session append failed: {error}"))
        })?;

        if let Some(waiter) = slot.remove_pending_question_waiter(&pending_request.request_id) {
            let _ = waiter.send(result);
        }

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

    async fn handle_queue_message(
        &mut self,
        session_id: &str,
        content: String,
    ) -> Result<QueueMessageResponse, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        // 检查是否正在处理队列（此时虽然是 Idle 但不应该开始新 turn）
        let is_processing_queue = slot.queue_processing;

        match slot.status() {
            SlotStatus::Idle if !is_processing_queue => {
                // 空闲时立即开始 turn
                let mut turn_execution =
                    TurnExecutionService::new(&mut self.slots, &self.config, &self.return_tx);
                let turn_id = turn_execution.submit_turn(session_id, vec![content]).await?;
                Ok(QueueMessageResponse {
                    status: QueueMessageStatus::Started,
                    turn_id: Some(turn_id),
                    position: None,
                    message_id: None,
                })
            }
            SlotStatus::Running | SlotStatus::Idle => {
                // 运行时或队列处理中，入队
                // 检查队列是否已满
                if slot.message_queue.len() >= MAX_QUEUE_SIZE {
                    return Err(RuntimeWorkerError::queue_full(MAX_QUEUE_SIZE));
                }

                // 生成消息 ID
                let message_id = generate_message_id();
                let queued_at_ms = now_timestamp_ms();

                // 注意：不在 Running 状态下直接写入文件，避免与 TurnWorker 的写入冲突
                // 消息队列会在 turn 结束后处理，那时候会正确写入 tape
                // 
                // 如果服务器在 turn 运行期间重启，内存中的队列会丢失，
                // 但这是可接受的，因为用户可以重新发送消息
                
                // 更新内存状态
                let position = slot.message_queue.len() as u32 + 1;
                slot.message_queue.push(QueuedMessage {
                    id: message_id.clone(),
                    content,
                    queued_at_ms,
                });

                // 广播 SSE 事件
                let _ = self.config.broadcast_tx.send(SsePayload::MessageQueued {
                    session_id: session_id.to_string(),
                    message_id: message_id.clone(),
                    position,
                    content_preview: slot.message_queue.last().unwrap().content.chars().take(50).collect(),
                });

                Ok(QueueMessageResponse {
                    status: QueueMessageStatus::Queued,
                    turn_id: None,
                    position: Some(position),
                    message_id: Some(message_id),
                })
            }
        }
    }

    fn handle_get_queue(
        &mut self,
        session_id: &str,
    ) -> Result<Vec<QueuedMessage>, RuntimeWorkerError> {
        let slot = self.slots.get(session_id).ok_or_else(|| {
            self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })
        })?;

        Ok(slot.message_queue.clone())
    }

    fn handle_delete_queued_message(
        &mut self,
        session_id: &str,
        message_id: &str,
    ) -> Result<(), RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        // 只能在 session 空闲时删除
        if slot.status() == SlotStatus::Running {
            return Err(RuntimeWorkerError::cannot_modify_queue_while_running());
        }

        // 查找并删除消息
        let index = slot
            .message_queue
            .iter()
            .position(|msg| msg.id == message_id)
            .ok_or_else(|| RuntimeWorkerError::message_not_found(message_id))?;

        slot.message_queue.remove(index);

        // 通过 runtime 追加删除事件到 tape（会有正确的 ID 分配）
        if let Some(runtime) = slot.runtime_mut() {
            let entry = TapeEntry::event("message_deleted", Some(serde_json::json!({
                "id": message_id
            })));
            runtime.append_tape_entry(entry).map_err(|error| {
                RuntimeWorkerError::internal(format!("session append failed: {error}"))
            })?;
        }

        // 广播 SSE 事件
        let _ = self.config.broadcast_tx.send(SsePayload::MessageDeleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            remaining_count: slot.message_queue.len() as u32,
        });

        Ok(())
    }

    fn handle_interrupt_turn(&mut self, session_id: &str) -> Result<bool, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        if slot.status() != SlotStatus::Running {
            return Ok(false);
        }

        // 设置中断标志
        slot.interrupt_requested = true;

        // 获取当前 turn_id
        let turn_id = slot
            .current_turn
            .read()
            .ok()
            .and_then(|current| current.as_ref().map(|turn| turn.turn_id.clone()));

        // 取消 turn
        if let Some(running_turn) = slot.running_turn() {
            running_turn.control.cancel();
        }

        // 取消 pending question（如果有）
        let _ = self.cancel_pending_question(session_id);

        // 广播 SSE 事件
        let _ = self.config.broadcast_tx.send(SsePayload::TurnInterrupted {
            session_id: session_id.to_string(),
            turn_id,
        });

        Ok(true)
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
        let mut tape = load_session_tape_with_repair(&session_path)?;
        reconcile_orphaned_inflight_state(&session_path, &mut tape)?;
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
        let mut runtime =
            AgentRuntime::with_tape(model, builtin_tools::build_tool_registry(), identity, tape)
                .with_instructions(build_session_system_prompt(
                    self.config.system_prompt.as_deref(),
                    &self.config.workspace_root,
                ))
                .with_hooks(self.config.runtime_hooks.clone())
                .with_runtime_tool_host(Arc::new(ServerRuntimeToolHost::new(
                    self.config.runtime_tool_host.clone(),
                )))
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

        // 恢复队列
        let message_queue = restore_queue_from_tape(runtime.tape());

        Ok(SessionSlot::idle(
            session_path,
            provider_binding,
            Arc::new(RwLock::new(snapshots.history)),
            Arc::new(RwLock::new(snapshots.current_turn)),
            Arc::new(RwLock::new(context_stats)),
            runtime,
            subscriber,
            message_queue,
        ))
    }
}

fn reconcile_orphaned_inflight_state(
    session_path: &std::path::Path,
    tape: &mut SessionTape,
) -> Result<(), RuntimeWorkerError> {
    let snapshots = rebuild_session_snapshots_from_tape(tape);
    let mut changed = false;

    if let Some(current_turn) = snapshots.current_turn {
        tape.append_entry(
            TapeEntry::event(
                "turn_failed",
                Some(serde_json::json!({
                    "message": "服务器重启，当前轮次已取消"
                })),
            )
            .with_run_id(&current_turn.turn_id),
        );
        changed = true;
    }

    let Some(pending_request) = tape
        .try_pending_question_request()
        .map_err(|error| RuntimeWorkerError::internal(error.to_string()))?
    else {
        if changed {
            return tape.save_jsonl(session_path).map_err(|error| {
                RuntimeWorkerError::internal(format!("session save failed: {error}"))
            });
        }
        return Ok(());
    };

    tape.record_question_resolved(&QuestionResult {
        status: QuestionResultStatus::Cancelled,
        request_id: pending_request.request_id,
        answers: Vec::new(),
        reason: Some("server restarted before the pending question could be resumed".to_string()),
    });
    changed = true;

    if !changed {
        return Ok(());
    }

    tape.save_jsonl(session_path)
        .map_err(|error| RuntimeWorkerError::internal(format!("session save failed: {error}")))
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

    let tape = load_session_tape_with_repair(&slot.session_path)?;
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

fn restore_queue_from_tape(tape: &SessionTape) -> Vec<QueuedMessage> {
    use std::collections::HashSet;
    
    let mut queue: Vec<QueuedMessage> = Vec::new();
    let mut deleted: HashSet<String> = HashSet::new();

    for entry in tape.entries() {
        if entry.kind != "event" {
            continue;
        }

        let event_name = entry.event_name();
        let event_data = entry.event_data();

        match event_name {
            Some("message_queued") => {
                if let Some(data) = event_data {
                    if let Ok(msg) = parse_queued_message(data) {
                        // 只有未被删除的才加入
                        if !deleted.contains(&msg.id) {
                            queue.push(msg);
                        }
                    }
                }
            }
            Some("message_deleted") | Some("message_dequeued") => {
                if let Some(id) = event_data.and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                    deleted.insert(id.to_string());
                    queue.retain(|m| m.id != id);
                }
            }
            _ => {}
        }
    }

    queue
}

fn parse_queued_message(data: &serde_json::Value) -> Result<QueuedMessage, ()> {
    Ok(QueuedMessage {
        id: data.get("id").and_then(|v| v.as_str()).ok_or(())?.to_string(),
        content: data.get("content").and_then(|v| v.as_str()).ok_or(())?.to_string(),
        queued_at_ms: data.get("queued_at_ms").and_then(|v| v.as_u64()).ok_or(())?,
    })
}

/// 创建一个用于丢弃回复的 oneshot channel
fn drop_reply_channel<T>() -> tokio::sync::oneshot::Sender<T> {
    let (tx, _rx) = tokio::sync::oneshot::channel();
    tx
}

pub(crate) use types::{read_lock, write_lock};
