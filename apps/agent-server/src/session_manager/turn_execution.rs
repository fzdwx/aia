use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use agent_core::{StreamEvent, ToolRegistry};
use agent_runtime::{AgentRuntime, ContextStats, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use tokio::sync::{broadcast, mpsc};

use crate::{
    model::ServerModel,
    runtime_worker::{CurrentTurnSnapshot, RunningTurnHandle, RuntimeWorkerError},
    sse::{SsePayload, TurnStatus},
};

use super::{
    CurrentStatusInner, RuntimeReturn, SessionId, SessionManagerConfig, SessionSlot, SlotStatus,
    ToolTraceRecorder, next_server_turn_id, now_timestamp_ms, update_current_turn_from_stream,
    update_current_turn_status, write_lock,
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

    pub(super) async fn submit_turn(
        &mut self,
        session_id: &str,
        prompt: String,
    ) -> Result<String, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        if slot.status == SlotStatus::Running {
            return Err(RuntimeWorkerError::bad_request(
                "a turn is already running in this session",
            ));
        }

        let runtime = slot
            .runtime
            .take()
            .ok_or_else(|| RuntimeWorkerError::internal("runtime not available"))?;
        *write_lock(&slot.context_stats) = runtime.context_stats();
        let subscriber = slot.subscriber;
        let turn_control = runtime.turn_control();
        slot.running_turn = Some(RunningTurnHandle { control: turn_control.clone() });
        slot.status = SlotStatus::Running;

        let _ = self.config.store.update_session_async(session_id.to_string(), None, None).await;

        let turn_id = next_server_turn_id();
        let current_turn = CurrentTurnSnapshot {
            turn_id: turn_id.clone(),
            started_at_ms: now_timestamp_ms(),
            user_message: prompt.clone(),
            status: TurnStatus::Waiting,
            blocks: Vec::new(),
        };
        *write_lock(&slot.current_turn) = Some(current_turn.clone());

        let session_id_owned = session_id.to_string();
        let worker = TurnWorker::new(
            runtime,
            subscriber,
            prompt,
            turn_control,
            TurnWorkerContext {
                broadcast_tx: self.config.broadcast_tx.clone(),
                current_turn_snapshot: slot.current_turn.clone(),
                history_snapshot: slot.history.clone(),
                context_stats_snapshot: slot.context_stats.clone(),
                trace_recorder: ToolTraceRecorder::new(self.config.store.clone()),
                session_id: session_id_owned.clone(),
                turn_id: turn_id.clone(),
            },
        );
        let return_tx = self.return_tx.clone();

        let _ = self.config.broadcast_tx.send(SsePayload::CurrentTurnStarted {
            session_id: session_id_owned.clone(),
            current_turn,
        });
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
}

pub(super) struct RuntimeEventProjector<'a> {
    broadcast_tx: &'a broadcast::Sender<SsePayload>,
    session_id: &'a str,
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

struct TurnWorkerContext {
    broadcast_tx: broadcast::Sender<SsePayload>,
    current_turn_snapshot: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    history_snapshot: Arc<RwLock<Vec<TurnLifecycle>>>,
    context_stats_snapshot: Arc<RwLock<ContextStats>>,
    trace_recorder: ToolTraceRecorder,
    session_id: SessionId,
    turn_id: String,
}

struct TurnWorker {
    runtime: AgentRuntime<ServerModel, ToolRegistry>,
    subscriber: RuntimeSubscriberId,
    prompt: String,
    turn_control: agent_runtime::TurnControl,
    context: TurnWorkerContext,
}

impl TurnWorker {
    fn new(
        runtime: AgentRuntime<ServerModel, ToolRegistry>,
        subscriber: RuntimeSubscriberId,
        prompt: String,
        turn_control: agent_runtime::TurnControl,
        context: TurnWorkerContext,
    ) -> Self {
        Self { runtime, subscriber, prompt, turn_control, context }
    }

    async fn run(mut self) -> RuntimeReturn {
        let mut current_status = CurrentStatusInner::Waiting;
        let status_broadcast = self.context.broadcast_tx.clone();
        let stream_session_id = self.context.session_id.clone();
        let stream_turn_id = self.context.turn_id.clone();
        let stream_snapshot = self.context.current_turn_snapshot.clone();

        let result = self
            .runtime
            .handle_turn_streaming(self.prompt.clone(), self.turn_control.clone(), |event| {
                let new_status = match &event {
                    StreamEvent::ThinkingDelta { .. } => CurrentStatusInner::Thinking,
                    StreamEvent::TextDelta { .. } => CurrentStatusInner::Generating,
                    StreamEvent::ToolCallDetected { .. } => current_status.clone(),
                    StreamEvent::ToolCallStarted { .. } => CurrentStatusInner::Working,
                    StreamEvent::ToolOutputDelta { .. } => CurrentStatusInner::Working,
                    StreamEvent::Done => CurrentStatusInner::Finishing,
                    _ => current_status.clone(),
                };

                if new_status != current_status {
                    current_status = new_status.clone();
                    update_current_turn_status(&stream_snapshot, new_status.to_turn_status());
                    let _ = status_broadcast.send(SsePayload::Status {
                        session_id: stream_session_id.clone(),
                        turn_id: stream_turn_id.clone(),
                        status: new_status.to_turn_status(),
                    });
                }

                update_current_turn_from_stream(&stream_snapshot, &event);
                let _ = status_broadcast.send(SsePayload::Stream {
                    session_id: stream_session_id.clone(),
                    turn_id: stream_turn_id.clone(),
                    event,
                });
            })
            .await;
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
                    write_lock(&self.context.history_snapshot).push(turn.clone());
                    let _ = self.context.broadcast_tx.send(SsePayload::TurnCompleted {
                        session_id: self.context.session_id.clone(),
                        turn_id: self.context.turn_id.clone(),
                        turn,
                    });
                    let trace_recorder = self.context.trace_recorder.clone();
                    tokio::spawn(async move {
                        trace_recorder.persist_turn_spans(&turn_for_traces).await;
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
}
