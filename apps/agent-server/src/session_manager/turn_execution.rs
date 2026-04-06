use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use agent_core::{StreamEvent, ToolRegistry, WidgetHostCommand};
use agent_runtime::{AgentRuntime, ContextStats, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use agent_store::SessionRecord;
use tokio::sync::{broadcast, mpsc};

use crate::{
    model::ServerModel,
    runtime_worker::{CurrentTurnSnapshot, RuntimeWorkerError},
    sse::{SsePayload, TurnStatus},
};

use super::{
    CurrentStatusInner, RuntimeReturn, SessionAutoRenameService, SessionId, SessionManagerConfig,
    SessionSlot, ToolTraceRecorder, next_server_turn_id, now_timestamp_ms, read_lock,
    update_current_turn_from_stream, update_current_turn_status, write_lock,
};

pub(super) struct TurnExecutionService<'a> {
    slots: &'a mut HashMap<SessionId, SessionSlot>,
    config: &'a SessionManagerConfig,
    return_tx: &'a mpsc::Sender<RuntimeReturn>,
}

impl<'a> TurnExecutionService<'a> {
    pub(super) fn new(
        slots: &'a mut HashMap<SessionId, SessionSlot>,
        config: &'a SessionManagerConfig,
        return_tx: &'a mpsc::Sender<RuntimeReturn>,
    ) -> Self {
        Self { slots, config, return_tx }
    }

    /// 提交 turn，支持单条或多条用户消息
    ///
    /// 每条消息作为独立的 user message 追加到 tape
    pub(super) async fn submit_turn(
        &mut self,
        session_id: &str,
        prompts: Vec<String>,
    ) -> Result<String, RuntimeWorkerError> {
        if prompts.is_empty() {
            return Err(RuntimeWorkerError::bad_request("no prompts provided"));
        }
        self.submit_turn_inner(session_id, prompts, Vec::new(), false).await
    }

    pub(super) async fn retry_turn(
        &mut self,
        session_id: &str,
        failed_turn_id: &str,
    ) -> Result<String, RuntimeWorkerError> {
        let user_messages = {
            let slot = self.slots.get_mut(session_id).ok_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })?;

            if slot.status() != super::SlotStatus::Idle {
                return Err(RuntimeWorkerError::bad_request(
                    "a turn is already running in this session",
                ));
            }

            let history = super::read_lock(&slot.history);
            let turn =
                history.iter().find(|candidate| candidate.turn_id == failed_turn_id).ok_or_else(
                    || RuntimeWorkerError::not_found(format!("turn not found: {failed_turn_id}")),
                )?;

            if turn.outcome != agent_runtime::TurnOutcome::Failed
                && turn.outcome != agent_runtime::TurnOutcome::Cancelled
            {
                return Err(RuntimeWorkerError::bad_request(
                    "only failed or cancelled turns can be retried",
                ));
            }

            let latest_retriable_turn_id = history
                .iter()
                .rev()
                .find(|candidate| {
                    candidate.outcome == agent_runtime::TurnOutcome::Failed
                        || candidate.outcome == agent_runtime::TurnOutcome::Cancelled
                })
                .map(|candidate| candidate.turn_id.clone());

            if latest_retriable_turn_id.as_deref() != Some(failed_turn_id) {
                return Err(RuntimeWorkerError::bad_request(
                    "only the latest failed or cancelled turn can be retried",
                ));
            }

            turn.user_messages.clone()
        };

        self.cleanup_failed_turn_tail(session_id, failed_turn_id)?;

        // cleanup 后 rebuild 会把旧 turn 的 entries 视为 "进行中"（current_turn），
        // 读取其 blocks 作为继续 turn 的初始状态
        let inherited_blocks = {
            let slot = self.slots.get(session_id).ok_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })?;
            super::read_lock(&slot.current_turn)
                .as_ref()
                .map(|snapshot| snapshot.blocks.clone())
                .unwrap_or_default()
        };

        self.submit_turn_inner(session_id, user_messages, inherited_blocks, true).await
    }

    async fn submit_turn_inner(
        &mut self,
        session_id: &str,
        prompts: Vec<String>,
        initial_blocks: Vec<crate::runtime_worker::CurrentTurnBlock>,
        continue_mode: bool,
    ) -> Result<String, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        slot.queue_processing = false;

        let (runtime, subscriber, running_turn) = slot.begin_turn()?;
        *write_lock(&slot.context_stats) = runtime.context_stats();
        let turn_control = running_turn.control.clone();

        let activity_record = self
            .config
            .store
            .touch_session_last_active_async(session_id.to_string())
            .await
            .map_err(|error| {
                RuntimeWorkerError::internal(format!("session activity update failed: {error}"))
            })?
            .ok_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })?;

        let turn_id = next_server_turn_id();
        let current_turn = CurrentTurnSnapshot {
            turn_id: turn_id.clone(),
            started_at_ms: now_timestamp_ms(),
            user_messages: prompts.clone(),
            status: TurnStatus::Waiting,
            blocks: initial_blocks,
        };
        *write_lock(&slot.current_turn) = Some(current_turn.clone());

        let session_id_owned = session_id.to_string();
        let worker = TurnWorker {
            runtime,
            subscriber,
            prompts,
            turn_control,
            context: TurnWorkerContext {
                broadcast_tx: self.config.broadcast_tx.clone(),
                current_turn_snapshot: slot.current_turn.clone(),
                history_snapshot: slot.history.clone(),
                context_stats_snapshot: slot.context_stats.clone(),
                trace_recorder: ToolTraceRecorder::new(self.config.store.clone()),
                registry: self.config.registry.clone(),
                sessions_dir: self.config.sessions_dir.clone(),
                session_id: session_id_owned.clone(),
                turn_id: turn_id.clone(),
            },
            continue_mode,
        };
        let return_tx = self.return_tx.clone();

        let _ = self.config.broadcast_tx.send(SsePayload::CurrentTurnStarted {
            session_id: session_id_owned.clone(),
            current_turn,
        });
        let _ = self.config.broadcast_tx.send(session_updated_payload(activity_record));
        let _ = self.config.broadcast_tx.send(SsePayload::Status {
            session_id: session_id_owned,
            turn_id: turn_id.clone(),
            status: TurnStatus::Waiting,
        });

        tokio::spawn(async move {
            let runtime_return = worker.run().await;
            let _ = return_tx.send(runtime_return).await;
        });

        Ok(turn_id)
    }

    /// 清理失败 turn 的尾部：移除 turn_failed 事件、孤立的 tool_call（无配对 tool_result）、
    /// 以及最后一个完整 tool_call/tool_result 对之后的 trailing thinking/message entries。
    fn cleanup_failed_turn_tail(
        &mut self,
        session_id: &str,
        failed_turn_id: &str,
    ) -> Result<(), RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        let target_turn = super::read_lock(&slot.history)
            .iter()
            .find(|candidate| candidate.turn_id == failed_turn_id)
            .cloned()
            .ok_or_else(|| {
                RuntimeWorkerError::not_found(format!("turn not found: {failed_turn_id}"))
            })?;

        let session_path = slot.session_path.clone();
        let source_ids: BTreeSet<u64> = target_turn.source_entry_ids.iter().copied().collect();

        let (rebuilt, context_stats) = {
            let runtime = slot
                .runtime_mut()
                .ok_or_else(|| RuntimeWorkerError::bad_request("session is not currently idle"))?;

            let mut tool_call_invocations: Vec<(String, u64)> = Vec::new();
            let mut tool_result_invocations: BTreeSet<String> = BTreeSet::new();
            let mut removable = BTreeSet::new();

            for entry in runtime.tape().entries() {
                if !source_ids.contains(&entry.id) {
                    continue;
                }
                // turn_failed / turn_completed 事件始终移除
                if entry.kind == "event" {
                    if let Some(name) = entry.event_name() {
                        if name == "turn_failed" || name == "turn_completed" {
                            removable.insert(entry.id);
                        }
                    }
                    continue;
                }
                if let Some(call) = entry.as_tool_call() {
                    tool_call_invocations.push((call.invocation_id.clone(), entry.id));
                }
                if let Some(result) = entry.as_tool_result() {
                    tool_result_invocations.insert(result.invocation_id.clone());
                }
            }

            // 孤立的 tool_call（没有配对 tool_result）
            for (invocation_id, entry_id) in &tool_call_invocations {
                if !tool_result_invocations.contains(invocation_id) {
                    removable.insert(*entry_id);
                }
            }

            // 找到最后一个已完成的 tool_result 或 user message 的 entry_id 作为截止点，
            // 之后属于该 turn 的 thinking/message entries 都是失败 LLM call 的残留
            let cutoff = runtime
                .tape()
                .entries()
                .iter()
                .rev()
                .find(|entry| {
                    source_ids.contains(&entry.id)
                        && (entry.kind == "tool_result"
                            || (entry.kind == "message"
                                && entry
                                    .as_message()
                                    .is_some_and(|msg| msg.role == agent_core::Role::User)))
                })
                .map(|entry| entry.id);

            if let Some(cutoff) = cutoff {
                for entry in runtime.tape().entries() {
                    if !source_ids.contains(&entry.id) || entry.id <= cutoff {
                        continue;
                    }
                    if matches!(entry.kind.as_str(), "thinking" | "message") {
                        removable.insert(entry.id);
                    }
                }
            }

            if !removable.is_empty() {
                runtime.tape_mut().remove_entries_by_id(&removable);
            }

            runtime.tape().save_jsonl(&session_path).map_err(|error| {
                RuntimeWorkerError::internal(format!("session save failed: {error}"))
            })?;

            let rebuilt =
                crate::runtime_worker::rebuild_session_snapshots_from_tape(runtime.tape());
            let context_stats = runtime.context_stats();
            (rebuilt, context_stats)
        };

        *super::write_lock(&slot.history) = rebuilt.history;
        *super::write_lock(&slot.current_turn) = rebuilt.current_turn;
        *super::write_lock(&slot.context_stats) = context_stats;

        Ok(())
    }
}

pub(super) struct RuntimeEventProjector<'a> {
    broadcast_tx: &'a broadcast::Sender<SsePayload>,
    session_id: &'a str,
}

fn session_updated_payload(record: SessionRecord) -> SsePayload {
    SsePayload::SessionUpdated {
        session_id: record.id,
        title: record.title,
        title_source: record.title_source,
        auto_rename_policy: record.auto_rename_policy,
        updated_at: record.updated_at,
        last_active_at: record.last_active_at,
        model: record.model,
    }
}

impl<'a> RuntimeEventProjector<'a> {
    pub(super) fn new(
        broadcast_tx: &'a broadcast::Sender<SsePayload>,
        session_id: &'a str,
    ) -> Self {
        Self { broadcast_tx, session_id }
    }

    pub(super) fn project(self, events: Vec<RuntimeEvent>) -> Option<TurnLifecycle> {
        let mut turn = None;
        for event in events {
            match event {
                RuntimeEvent::TurnLifecycle { turn: lifecycle } => turn = Some(lifecycle),
                RuntimeEvent::ContextCompressed { summary } => {
                    let _ = self.broadcast_tx.send(SsePayload::ContextCompressed {
                        session_id: self.session_id.to_string(),
                        summary,
                    });
                }
                _ => {}
            }
        }
        turn
    }
}

pub(super) fn collect_runtime_events(
    runtime: &mut AgentRuntime<ServerModel, ToolRegistry>,
    subscriber: RuntimeSubscriberId,
) -> Result<Vec<RuntimeEvent>, RuntimeWorkerError> {
    runtime.collect_events(subscriber).map_err(|error| {
        RuntimeWorkerError::internal(format!("runtime event collection failed: {error}"))
    })
}

pub(super) struct TurnWorkerContext {
    pub(super) broadcast_tx: broadcast::Sender<SsePayload>,
    pub(super) current_turn_snapshot: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    pub(super) history_snapshot: Arc<RwLock<Vec<TurnLifecycle>>>,
    pub(super) context_stats_snapshot: Arc<RwLock<ContextStats>>,
    pub(super) trace_recorder: ToolTraceRecorder,
    pub(super) registry: provider_registry::ProviderRegistry,
    pub(super) sessions_dir: std::path::PathBuf,
    pub(super) session_id: SessionId,
    pub(super) turn_id: String,
}

pub(super) struct TurnWorker {
    runtime: AgentRuntime<ServerModel, ToolRegistry>,
    subscriber: RuntimeSubscriberId,
    prompts: Vec<String>,
    turn_control: agent_runtime::TurnControl,
    context: TurnWorkerContext,
    continue_mode: bool,
}

impl TurnWorker {
    pub(super) async fn run(mut self) -> RuntimeReturn {
        let mut current_status = CurrentStatusInner::Waiting;
        let status_broadcast = self.context.broadcast_tx.clone();
        let stream_session_id = self.context.session_id.clone();
        let stream_turn_id = self.context.turn_id.clone();
        let stream_snapshot = self.context.current_turn_snapshot.clone();

        let prompts = std::mem::take(&mut self.prompts);
        let turn_control = self.turn_control.clone();
        let continue_mode = self.continue_mode;
        let result = if continue_mode {
            self.runtime
                .handle_continue_streaming(prompts, turn_control, |event| {
                    Self::handle_stream_event(
                        &event,
                        &mut current_status,
                        &status_broadcast,
                        &stream_session_id,
                        &stream_turn_id,
                        &stream_snapshot,
                    );
                })
                .await
        } else {
            self.runtime
                .handle_turn_streaming(prompts, turn_control, |event| {
                    Self::handle_stream_event(
                        &event,
                        &mut current_status,
                        &status_broadcast,
                        &stream_session_id,
                        &stream_turn_id,
                        &stream_snapshot,
                    );
                })
                .await
        };
        *write_lock(&self.context.context_stats_snapshot) = self.runtime.context_stats();

        match result {
            Ok(_) => self.handle_terminal_events(None).await,
            Err(error) => self.handle_terminal_events(Some(error)).await,
        }

        RuntimeReturn {
            session_id: self.context.session_id,
            runtime: self.runtime,
            subscriber: self.subscriber,
        }
    }

    async fn handle_terminal_events(&mut self, error: Option<agent_runtime::RuntimeError>) {
        let collected = collect_runtime_events(&mut self.runtime, self.subscriber);
        match collected {
            Ok(events) => {
                let turn = RuntimeEventProjector::new(
                    &self.context.broadcast_tx,
                    &self.context.session_id,
                )
                .project(events);
                if let Some(turn) = turn {
                    let turn_for_traces = turn.clone();
                    {
                        let mut history = write_lock(&self.context.history_snapshot);
                        if let Some(index) =
                            history.iter().position(|item| item.turn_id == turn.turn_id)
                        {
                            history[index] = turn.clone();
                        } else {
                            history.push(turn.clone());
                        }
                    }
                    let _ = self.context.broadcast_tx.send(SsePayload::TurnCompleted {
                        session_id: self.context.session_id.clone(),
                        turn_id: self.context.turn_id.clone(),
                        turn: turn.clone(),
                    });
                    let trace_recorder = self.context.trace_recorder.clone();
                    let turn_for_trace_persist = turn_for_traces.clone();
                    tokio::spawn(async move {
                        trace_recorder.persist_turn_spans(&turn_for_trace_persist).await;
                    });
                    let auto_rename = SessionAutoRenameService {
                        store: self.context.trace_recorder.store(),
                        registry: self.context.registry.clone(),
                        broadcast_tx: self.context.broadcast_tx.clone(),
                        sessions_dir: self.context.sessions_dir.clone(),
                    };
                    let session_id = self.context.session_id.clone();
                    tokio::spawn(async move {
                        auto_rename
                            .maybe_schedule_after_turn(&session_id, &turn_for_traces, true)
                            .await;
                    });
                }
            }
            Err(collection_error) => {
                let _ = self.context.broadcast_tx.send(SsePayload::Error {
                    session_id: self.context.session_id.clone(),
                    turn_id: Some(self.context.turn_id.clone()),
                    message: collection_error.message.clone(),
                });
            }
        }

        if let Some(error) = error {
            if error.is_cancelled() {
                update_current_turn_status(
                    &self.context.current_turn_snapshot,
                    TurnStatus::Cancelled,
                );
                let _ = self.context.broadcast_tx.send(SsePayload::Status {
                    session_id: self.context.session_id.clone(),
                    turn_id: self.context.turn_id.clone(),
                    status: TurnStatus::Cancelled,
                });
                let _ = self.context.broadcast_tx.send(SsePayload::TurnCancelled {
                    session_id: self.context.session_id.clone(),
                    turn_id: self.context.turn_id.clone(),
                });
            }

            let _ = self.context.broadcast_tx.send(SsePayload::Error {
                session_id: self.context.session_id.clone(),
                turn_id: Some(self.context.turn_id.clone()),
                message: error.to_string(),
            });
        }

        *write_lock(&self.context.current_turn_snapshot) = None;
    }

    fn handle_stream_event(
        event: &StreamEvent,
        current_status: &mut CurrentStatusInner,
        status_broadcast: &broadcast::Sender<SsePayload>,
        stream_session_id: &str,
        stream_turn_id: &str,
        stream_snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    ) {
        fn widget_from_snapshot(
            snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
            invocation_id: &str,
        ) -> Option<agent_core::UiWidget> {
            let guard = read_lock(snapshot);
            let current = guard.as_ref()?;
            current.blocks.iter().rev().find_map(|block| match block {
                crate::runtime_worker::CurrentTurnBlock::Tool { tool }
                    if tool.invocation_id == invocation_id =>
                {
                    tool.widget.clone()
                }
                _ => None,
            })
        }

        let new_status = match event {
            StreamEvent::ThinkingDelta { .. } => CurrentStatusInner::Thinking,
            StreamEvent::TextDelta { .. } => CurrentStatusInner::Generating,
            StreamEvent::ToolCallDetected { .. } => current_status.clone(),
            StreamEvent::ToolCallArgumentsDelta { .. } => current_status.clone(),
            StreamEvent::ToolCallReady { .. } => current_status.clone(),
            StreamEvent::ToolCallStarted { .. } => CurrentStatusInner::Working,
            StreamEvent::ToolOutputDelta { .. } => CurrentStatusInner::Working,
            StreamEvent::WidgetHostCommand { .. } => current_status.clone(),
            StreamEvent::WidgetClientEvent { .. } => current_status.clone(),
            StreamEvent::Retrying { .. } => CurrentStatusInner::Retrying,
            StreamEvent::Done => CurrentStatusInner::Finishing,
            _ => current_status.clone(),
        };

        if new_status != *current_status {
            *current_status = new_status.clone();
            update_current_turn_status(stream_snapshot, new_status.to_turn_status());
            let _ = status_broadcast.send(SsePayload::Status {
                session_id: stream_session_id.to_string(),
                turn_id: stream_turn_id.to_string(),
                status: new_status.to_turn_status(),
            });
        }

        update_current_turn_from_stream(stream_snapshot, event);
        let widget = match event {
            StreamEvent::ToolCallDetected { invocation_id, .. }
            | StreamEvent::ToolCallArgumentsDelta { invocation_id, .. }
            | StreamEvent::ToolCallReady { call: agent_core::ToolCall { invocation_id, .. } }
            | StreamEvent::ToolCallStarted { invocation_id, .. }
            | StreamEvent::ToolOutputDelta { invocation_id, .. }
            | StreamEvent::ToolCallCompleted { invocation_id, .. } => {
                widget_from_snapshot(stream_snapshot, invocation_id)
            }
            _ => None,
        };
        let _ = status_broadcast.send(SsePayload::Stream {
            session_id: stream_session_id.to_string(),
            turn_id: stream_turn_id.to_string(),
            event: event.clone(),
            widget,
        });

        if let Some((invocation_id, widget)) = match event {
            StreamEvent::ToolCallDetected { invocation_id, .. }
            | StreamEvent::ToolCallArgumentsDelta { invocation_id, .. }
            | StreamEvent::ToolCallReady { call: agent_core::ToolCall { invocation_id, .. } }
            | StreamEvent::ToolCallStarted { invocation_id, .. }
            | StreamEvent::ToolOutputDelta { invocation_id, .. }
            | StreamEvent::ToolCallCompleted { invocation_id, .. } => {
                widget_from_snapshot(stream_snapshot, invocation_id)
                    .map(|widget| (invocation_id.clone(), widget))
            }
            _ => None,
        } {
            let _ = status_broadcast.send(SsePayload::Stream {
                session_id: stream_session_id.to_string(),
                turn_id: stream_turn_id.to_string(),
                event: StreamEvent::WidgetHostCommand {
                    invocation_id,
                    command: WidgetHostCommand::Render { widget: widget.clone() },
                },
                widget: Some(widget),
            });
        }
    }
}
