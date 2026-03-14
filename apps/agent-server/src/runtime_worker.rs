use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, mpsc};
use std::thread;

use agent_core::{ModelIdentity, Role, StreamEvent};
use agent_runtime::{AgentRuntime, ContextStats, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use axum::http::StatusCode;
use llm_trace::LlmTraceStore;
use provider_registry::{ModelConfig, ProviderKind, ProviderProfile, ProviderRegistry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use session_tape::{SessionProviderBinding, SessionTape};
use tokio::sync::{broadcast, oneshot};

use crate::{
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    sse::{SsePayload, TurnStatus},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentToolOutput {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub output: String,
    pub completed: bool,
    pub result_content: Option<String>,
    pub result_details: Option<serde_json::Value>,
    pub failed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurrentTurnBlock {
    Thinking { content: String },
    Tool { tool: CurrentToolOutput },
    Text { content: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentTurnSnapshot {
    pub started_at_ms: u64,
    pub user_message: String,
    pub status: TurnStatus,
    pub blocks: Vec<CurrentTurnBlock>,
}

#[derive(Default)]
pub struct SessionSnapshots {
    pub history: Vec<TurnLifecycle>,
    pub current_turn: Option<CurrentTurnSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfoSnapshot {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

impl ProviderInfoSnapshot {
    pub fn from_identity(identity: &ModelIdentity) -> Self {
        Self { name: identity.provider.clone(), model: identity.name.clone(), connected: true }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeWorkerError {
    pub status: StatusCode,
    pub message: String,
}

impl RuntimeWorkerError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn unavailable() -> Self {
        Self::internal("runtime worker unavailable")
    }
}

#[derive(Clone)]
pub struct RuntimeWorkerHandle {
    tx: mpsc::Sender<RuntimeCommand>,
}

impl RuntimeWorkerHandle {
    pub fn submit_turn(&self, prompt: String) -> Result<(), RuntimeWorkerError> {
        self.tx
            .send(RuntimeCommand::SubmitTurn { prompt })
            .map_err(|_| RuntimeWorkerError::unavailable())
    }

    pub async fn get_session_info(&self) -> Result<ContextStats, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::GetSessionInfo { reply: reply_tx })
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())
    }

    pub async fn create_handoff(
        &self,
        name: String,
        summary: String,
    ) -> Result<u64, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::CreateHandoff { name, summary, reply: reply_tx })
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn create_provider(
        &self,
        input: CreateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::CreateProvider { input, reply: reply_tx })
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
            .send(RuntimeCommand::UpdateProvider { name, input, reply: reply_tx })
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn delete_provider(&self, name: String) -> Result<(), RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::DeleteProvider { name, reply: reply_tx })
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }

    pub async fn switch_provider(
        &self,
        input: SwitchProviderInput,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::SwitchProvider { input, reply: reply_tx })
            .map_err(|_| RuntimeWorkerError::unavailable())?;
        reply_rx.await.map_err(|_| RuntimeWorkerError::unavailable())?
    }
}

pub struct RuntimeOwnerState {
    pub runtime: AgentRuntime<ServerModel, agent_core::ToolRegistry>,
    pub subscriber: RuntimeSubscriberId,
    pub session_path: PathBuf,
    pub registry: ProviderRegistry,
    pub store_path: PathBuf,
    pub trace_store: Arc<dyn LlmTraceStore>,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub history_snapshot: Arc<RwLock<Vec<TurnLifecycle>>>,
    pub current_turn_snapshot: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
}

#[derive(Clone)]
pub struct CreateProviderInput {
    pub name: String,
    pub kind: ProviderKind,
    pub models: Vec<ModelConfig>,
    pub active_model: Option<String>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct UpdateProviderInput {
    pub kind: Option<ProviderKind>,
    pub models: Option<Vec<ModelConfig>>,
    pub active_model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct SwitchProviderInput {
    pub name: String,
    pub model_id: Option<String>,
}

enum RuntimeCommand {
    SubmitTurn {
        prompt: String,
    },
    GetSessionInfo {
        reply: oneshot::Sender<ContextStats>,
    },
    CreateHandoff {
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

pub fn spawn_runtime_worker(mut owner: RuntimeOwnerState) -> RuntimeWorkerHandle {
    refresh_provider_snapshots(&owner.registry, owner.runtime.model_identity(), &owner);
    refresh_session_snapshots(owner.runtime.tape(), &owner);

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(command) = rx.recv() {
            match command {
                RuntimeCommand::SubmitTurn { prompt } => {
                    run_turn(&mut owner, prompt);
                }
                RuntimeCommand::GetSessionInfo { reply } => {
                    let _ = reply.send(owner.runtime.context_stats());
                }
                RuntimeCommand::CreateHandoff { name, summary, reply } => {
                    let result = create_handoff(&mut owner, name, summary);
                    let _ = reply.send(result);
                }
                RuntimeCommand::CreateProvider { input, reply } => {
                    let result = create_provider(&mut owner, input);
                    let _ = reply.send(result);
                }
                RuntimeCommand::UpdateProvider { name, input, reply } => {
                    let result = update_provider(&mut owner, name, input);
                    let _ = reply.send(result);
                }
                RuntimeCommand::DeleteProvider { name, reply } => {
                    let result = delete_provider(&mut owner, name);
                    let _ = reply.send(result);
                }
                RuntimeCommand::SwitchProvider { input, reply } => {
                    let result = switch_provider(&mut owner, input);
                    let _ = reply.send(result);
                }
            }
        }
    });
    RuntimeWorkerHandle { tx }
}

fn create_handoff(
    owner: &mut RuntimeOwnerState,
    name: String,
    summary: String,
) -> Result<u64, RuntimeWorkerError> {
    let handoff = owner.runtime.handoff(
        &name,
        json!({
            "phase": name,
            "summary": summary,
            "next_steps": [],
            "source_entry_ids": [],
            "owner": "user"
        }),
    );
    owner
        .runtime
        .tape()
        .save_jsonl(&owner.session_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("session save failed: {e}")))?;
    Ok(handoff.anchor.entry_id)
}

fn create_provider(
    owner: &mut RuntimeOwnerState,
    input: CreateProviderInput,
) -> Result<(), RuntimeWorkerError> {
    let mut candidate_registry = owner.registry.clone();
    let active_model = input.active_model.or_else(|| input.models.first().map(|m| m.id.clone()));
    candidate_registry.upsert(ProviderProfile {
        name: input.name,
        kind: input.kind,
        base_url: input.base_url,
        api_key: input.api_key,
        models: input.models,
        active_model,
    });
    sync_runtime_to_registry(owner, candidate_registry).map(|_| ())
}

fn update_provider(
    owner: &mut RuntimeOwnerState,
    name: String,
    input: UpdateProviderInput,
) -> Result<(), RuntimeWorkerError> {
    let profile = owner
        .registry
        .providers()
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("provider 不存在：{name}")))?;

    let updated = ProviderProfile {
        name: name.clone(),
        kind: input.kind.unwrap_or(profile.kind),
        base_url: input.base_url.unwrap_or(profile.base_url),
        api_key: input.api_key.unwrap_or(profile.api_key),
        models: input.models.unwrap_or(profile.models),
        active_model: input.active_model.or(profile.active_model),
    };

    let mut candidate_registry = owner.registry.clone();
    candidate_registry.upsert(updated);
    sync_runtime_to_registry(owner, candidate_registry).map(|_| ())
}

fn delete_provider(owner: &mut RuntimeOwnerState, name: String) -> Result<(), RuntimeWorkerError> {
    let mut candidate_registry = owner.registry.clone();
    candidate_registry.remove(&name).map_err(|e| RuntimeWorkerError::not_found(e.to_string()))?;
    sync_runtime_to_registry(owner, candidate_registry).map(|_| ())
}

fn switch_provider(
    owner: &mut RuntimeOwnerState,
    input: SwitchProviderInput,
) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
    let mut profile =
        owner.registry.providers().iter().find(|p| p.name == input.name).cloned().ok_or_else(
            || RuntimeWorkerError::not_found(format!("provider 不存在：{}", input.name)),
        )?;

    if let Some(model_id) = &input.model_id {
        if !profile.has_model(model_id) {
            return Err(RuntimeWorkerError::bad_request(format!("模型不存在：{model_id}")));
        }
        profile.active_model = Some(model_id.clone());
    }

    let mut candidate_registry = owner.registry.clone();
    candidate_registry.upsert(profile);
    candidate_registry
        .set_active(&input.name)
        .map_err(|e| RuntimeWorkerError::bad_request(e.to_string()))?;
    sync_runtime_to_registry(owner, candidate_registry)
}

fn run_turn(owner: &mut RuntimeOwnerState, prompt: String) {
    let broadcast_tx = owner.broadcast_tx.clone();
    let current_turn_snapshot = owner.current_turn_snapshot.clone();
    let history_snapshot = owner.history_snapshot.clone();
    *current_turn_snapshot.write().expect("lock poisoned") = Some(CurrentTurnSnapshot {
        started_at_ms: now_timestamp_ms(),
        user_message: prompt.clone(),
        status: TurnStatus::Waiting,
        blocks: Vec::new(),
    });
    let _ = broadcast_tx.send(SsePayload::Status(TurnStatus::Waiting));

    let mut current_status = CurrentStatus::Waiting;
    let btx = broadcast_tx.clone();
    let result = owner.runtime.handle_turn_streaming(prompt, |event| {
        let new_status = match &event {
            StreamEvent::ThinkingDelta { .. } => CurrentStatus::Thinking,
            StreamEvent::TextDelta { .. } => CurrentStatus::Generating,
            StreamEvent::ToolCallStarted { .. } => CurrentStatus::Working,
            StreamEvent::ToolOutputDelta { .. } => CurrentStatus::Working,
            _ => current_status.clone(),
        };

        if new_status != current_status {
            current_status = new_status.clone();
            update_current_turn_status(&current_turn_snapshot, new_status.to_turn_status());
            let _ = btx.send(SsePayload::Status(new_status.to_turn_status()));
        }

        update_current_turn_from_stream(&current_turn_snapshot, &event);
        let _ = btx.send(SsePayload::Stream(event));
    });

    match result {
        Ok(_) => {
            let events = owner.runtime.collect_events(owner.subscriber).unwrap_or_default();
            let turn = broadcast_runtime_events(events, &broadcast_tx);
            if let Some(turn) = turn {
                history_snapshot.write().expect("lock poisoned").push(turn.clone());
                let _ = broadcast_tx.send(SsePayload::TurnCompleted(turn));
            }
            *current_turn_snapshot.write().expect("lock poisoned") = None;
        }
        Err(error) => {
            let events = owner.runtime.collect_events(owner.subscriber).unwrap_or_default();
            let turn = broadcast_runtime_events(events, &broadcast_tx);
            if let Some(turn) = turn {
                history_snapshot.write().expect("lock poisoned").push(turn.clone());
                let _ = broadcast_tx.send(SsePayload::TurnCompleted(turn));
            }
            *current_turn_snapshot.write().expect("lock poisoned") = None;
            let _ = broadcast_tx.send(SsePayload::Error(error.to_string()));
        }
    }
}

fn refresh_provider_snapshots(
    registry: &ProviderRegistry,
    identity: &ModelIdentity,
    owner: &RuntimeOwnerState,
) {
    *owner.provider_registry_snapshot.write().expect("lock poisoned") = registry.clone();
    *owner.provider_info_snapshot.write().expect("lock poisoned") =
        ProviderInfoSnapshot::from_identity(identity);
}

fn refresh_session_snapshots(tape: &SessionTape, owner: &RuntimeOwnerState) {
    let snapshots = rebuild_session_snapshots_from_tape(tape);
    *owner.history_snapshot.write().expect("lock poisoned") = snapshots.history;
    *owner.current_turn_snapshot.write().expect("lock poisoned") = snapshots.current_turn;
}

fn now_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u64::MAX as u128) as u64,
        Err(_) => 0,
    }
}

fn object_value(value: &serde_json::Value) -> serde_json::Value {
    if value.is_object() { value.clone() } else { json!({}) }
}

fn update_current_turn_status(
    snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    status: TurnStatus,
) {
    if let Some(current) = snapshot.write().expect("lock poisoned").as_mut() {
        current.status = status;
    }
}

fn update_current_turn_from_stream(
    snapshot: &Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    event: &StreamEvent,
) {
    let mut current = snapshot.write().expect("lock poisoned");
    let Some(snapshot) = current.as_mut() else {
        return;
    };

    match event {
        StreamEvent::ThinkingDelta { text } => {
            match snapshot.blocks.last_mut() {
                Some(CurrentTurnBlock::Thinking { content }) => content.push_str(text),
                _ => snapshot.blocks.push(CurrentTurnBlock::Thinking { content: text.clone() }),
            }
        }
        StreamEvent::TextDelta { text } => {
            match snapshot.blocks.last_mut() {
                Some(CurrentTurnBlock::Text { content }) => content.push_str(text),
                _ => snapshot.blocks.push(CurrentTurnBlock::Text { content: text.clone() }),
            }
        }
        StreamEvent::ToolCallStarted { invocation_id, tool_name, arguments } => {
            if let Some(CurrentTurnBlock::Tool { tool }) = snapshot.blocks.iter_mut().rev().find(
                |block| {
                    matches!(
                        block,
                        CurrentTurnBlock::Tool { tool }
                            if tool.invocation_id == *invocation_id
                    )
                },
            ) {
                tool.tool_name = tool_name.clone();
                tool.arguments = object_value(arguments);
            } else {
                snapshot.blocks.push(CurrentTurnBlock::Tool {
                    tool: CurrentToolOutput {
                        invocation_id: invocation_id.clone(),
                        tool_name: tool_name.clone(),
                        arguments: object_value(arguments),
                        started_at_ms: now_timestamp_ms(),
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
            if let Some(CurrentTurnBlock::Tool { tool }) = snapshot.blocks.iter_mut().rev().find(
                |block| {
                    matches!(
                        block,
                        CurrentTurnBlock::Tool { tool }
                            if tool.invocation_id == *invocation_id
                    )
                },
            ) {
                tool.output.push_str(text);
            } else {
                snapshot.blocks.push(CurrentTurnBlock::Tool {
                    tool: CurrentToolOutput {
                        invocation_id: invocation_id.clone(),
                        tool_name: String::new(),
                        arguments: json!({}),
                        started_at_ms: now_timestamp_ms(),
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
        StreamEvent::ToolCallCompleted {
            invocation_id,
            tool_name,
            content,
            details,
            failed,
        } => {
            if let Some(CurrentTurnBlock::Tool { tool }) = snapshot.blocks.iter_mut().rev().find(
                |block| {
                    matches!(
                        block,
                        CurrentTurnBlock::Tool { tool }
                            if tool.invocation_id == *invocation_id
                    )
                },
            ) {
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

fn sync_runtime_to_registry(
    owner: &mut RuntimeOwnerState,
    candidate_registry: ProviderRegistry,
) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
    let (info, identity, model, binding) =
        prepare_runtime_sync(&candidate_registry, Some(owner.trace_store.clone()))?;
    let mut candidate_tape = owner.runtime.tape().clone();
    candidate_tape.bind_provider(binding);

    candidate_registry
        .save(&owner.store_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("provider registry save failed: {e}")))?;
    candidate_tape
        .save_jsonl(&owner.session_path)
        .map_err(|e| RuntimeWorkerError::internal(format!("session save failed: {e}")))?;

    owner.registry = candidate_registry;
    owner.runtime.replace_model(model, identity);
    *owner.runtime.tape_mut() = candidate_tape;
    refresh_provider_snapshots(&owner.registry, owner.runtime.model_identity(), owner);
    Ok(info)
}

#[derive(Default)]
struct TurnHistoryBuilder {
    turn_id: String,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    source_entry_ids: Vec<u64>,
    user_message: Option<String>,
    blocks: Vec<agent_runtime::TurnBlock>,
    assistant_message: Option<String>,
    thinking: Option<String>,
    tool_invocations: Vec<agent_runtime::ToolInvocationLifecycle>,
    failure_message: Option<String>,
    pending_tool_calls: BTreeMap<String, agent_core::ToolCall>,
    completed: bool,
}

impl TurnHistoryBuilder {
    fn new(turn_id: String) -> Self {
        Self { turn_id, ..Self::default() }
    }

    fn push_entry(&mut self, entry: &session_tape::TapeEntry) {
        self.source_entry_ids.push(entry.id);
        let timestamp_ms = parse_iso8601_utc_seconds(&entry.date).unwrap_or(0);
        self.started_at_ms = Some(self.started_at_ms.unwrap_or(timestamp_ms));
        self.finished_at_ms = Some(timestamp_ms);

        if let Some(message) = entry.as_message() {
            match message.role {
                Role::User => {
                    if self.user_message.is_none() {
                        self.user_message = Some(message.content);
                    }
                }
                Role::Assistant => {
                    self.assistant_message = Some(message.content.clone());
                    self.blocks
                        .push(agent_runtime::TurnBlock::Assistant { content: message.content });
                }
                Role::System | Role::Tool => {}
            }
            return;
        }

        if let Some(content) = entry.as_thinking() {
            match &mut self.thinking {
                Some(existing) => existing.push_str(content),
                None => self.thinking = Some(content.to_string()),
            }
            self.blocks.push(agent_runtime::TurnBlock::Thinking { content: content.to_string() });
            return;
        }

        if let Some(call) = entry.as_tool_call() {
            self.pending_tool_calls.insert(call.invocation_id.clone(), call);
            return;
        }

        if let Some(result) = entry.as_tool_result() {
            let call = self.pending_tool_calls.remove(&result.invocation_id).unwrap_or_else(|| {
                agent_core::ToolCall::new(result.tool_name.clone())
                    .with_invocation_id(result.invocation_id.clone())
            });
            let invocation = agent_runtime::ToolInvocationLifecycle {
                call,
                started_at_ms: timestamp_ms,
                finished_at_ms: timestamp_ms,
                outcome: agent_runtime::ToolInvocationOutcome::Succeeded { result },
            };
            self.blocks
                .push(agent_runtime::TurnBlock::ToolInvocation { invocation: invocation.clone() });
            self.tool_invocations.push(invocation);
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_failed") {
            let message = entry
                .event_data()
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .unwrap_or("turn failed")
                .to_string();
            self.failure_message = Some(message.clone());
            self.blocks.push(agent_runtime::TurnBlock::Failure { message });
            self.completed = true;
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_completed") {
            self.completed = true;
        }
    }

    fn into_turn_lifecycle(self) -> Option<TurnLifecycle> {
        let user_message = self.user_message?;
        Some(TurnLifecycle {
            turn_id: self.turn_id,
            started_at_ms: self.started_at_ms.unwrap_or(0),
            finished_at_ms: self.finished_at_ms.unwrap_or(0),
            source_entry_ids: self.source_entry_ids,
            user_message,
            blocks: self.blocks,
            assistant_message: self.assistant_message,
            thinking: self.thinking,
            tool_invocations: self.tool_invocations,
            failure_message: self.failure_message,
        })
    }

    fn is_completed(&self) -> bool {
        self.completed
    }

    fn into_current_turn(self) -> Option<CurrentTurnSnapshot> {
        let lifecycle = self.into_turn_lifecycle()?;
        let status = if lifecycle
            .blocks
            .iter()
            .any(|block| matches!(block, agent_runtime::TurnBlock::ToolInvocation { .. }))
        {
            TurnStatus::Working
        } else if lifecycle
            .blocks
            .iter()
            .any(|block| matches!(block, agent_runtime::TurnBlock::Assistant { .. }))
        {
            TurnStatus::Generating
        } else if lifecycle
            .blocks
            .iter()
            .any(|block| matches!(block, agent_runtime::TurnBlock::Thinking { .. }))
        {
            TurnStatus::Thinking
        } else {
            TurnStatus::Waiting
        };

        Some(CurrentTurnSnapshot {
            started_at_ms: lifecycle.started_at_ms,
            user_message: lifecycle.user_message,
            status,
            blocks: lifecycle
                .blocks
                .into_iter()
                .filter_map(|block| match block {
                    agent_runtime::TurnBlock::Thinking { content } => {
                        Some(CurrentTurnBlock::Thinking { content })
                    }
                    agent_runtime::TurnBlock::Assistant { content } => {
                        Some(CurrentTurnBlock::Text { content })
                    }
                    agent_runtime::TurnBlock::ToolInvocation { invocation } => {
                        let (result_content, result_details, failed) = match invocation.outcome {
                            agent_runtime::ToolInvocationOutcome::Succeeded { result } => (
                                Some(result.content),
                                result.details,
                                Some(false),
                            ),
                            agent_runtime::ToolInvocationOutcome::Failed { message } => {
                                (Some(message), None, Some(true))
                            }
                        };

                        Some(CurrentTurnBlock::Tool {
                            tool: CurrentToolOutput {
                                invocation_id: invocation.call.invocation_id,
                                tool_name: invocation.call.tool_name,
                                arguments: object_value(&invocation.call.arguments),
                                started_at_ms: invocation.started_at_ms,
                                finished_at_ms: Some(invocation.finished_at_ms),
                                output: String::new(),
                                completed: true,
                                result_content,
                                result_details,
                                failed,
                            },
                        })
                    }
                    agent_runtime::TurnBlock::Failure { .. } => None,
                })
                .collect(),
        })
    }
}

pub(crate) fn rebuild_session_snapshots_from_tape(tape: &SessionTape) -> SessionSnapshots {
    let mut builders = Vec::<TurnHistoryBuilder>::new();
    let mut by_run_id = BTreeMap::<String, usize>::new();
    let mut history = Vec::<TurnLifecycle>::new();

    for entry in tape.entries() {
        if entry.kind == "event" && entry.event_name() == Some("turn_record") {
            if let Some(turn) = parse_legacy_turn_record(entry) {
                history.push(turn);
            }
            continue;
        }

        let run_id = entry
            .meta
            .get("run_id")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(run_id) = run_id else {
            continue;
        };

        let index = match by_run_id.get(&run_id) {
            Some(index) => *index,
            None => {
                let index = builders.len();
                builders.push(TurnHistoryBuilder::new(run_id.clone()));
                by_run_id.insert(run_id, index);
                index
            }
        };
        builders[index].push_entry(entry);
    }

    let mut current_candidates = Vec::<CurrentTurnSnapshot>::new();
    for builder in builders {
        if builder.is_completed() {
            if let Some(turn) = builder.into_turn_lifecycle() {
                history.push(turn);
            }
        } else if let Some(current) = builder.into_current_turn() {
            current_candidates.push(current);
        }
    }

    history.sort_by_key(|turn| (turn.started_at_ms, turn.finished_at_ms, turn.turn_id.clone()));
    let current_turn = current_candidates.into_iter().max_by_key(|turn| turn.started_at_ms);
    SessionSnapshots { history, current_turn }
}

pub(crate) fn rebuild_turn_history_from_tape(tape: &SessionTape) -> Vec<TurnLifecycle> {
    rebuild_session_snapshots_from_tape(tape).history
}

fn parse_legacy_turn_record(entry: &session_tape::TapeEntry) -> Option<TurnLifecycle> {
    let data = entry.event_data()?.clone();
    serde_json::from_value(data).ok()
}

fn parse_iso8601_utc_seconds(input: &str) -> Option<u64> {
    if input.len() != 20 || !input.ends_with('Z') {
        return None;
    }

    let year: i64 = input.get(0..4)?.parse().ok()?;
    let month: i64 = input.get(5..7)?.parse().ok()?;
    let day: i64 = input.get(8..10)?.parse().ok()?;
    let hour: i64 = input.get(11..13)?.parse().ok()?;
    let minute: i64 = input.get(14..16)?.parse().ok()?;
    let second: i64 = input.get(17..19)?.parse().ok()?;

    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 { adjusted_year } else { adjusted_year - 399 } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days_since_epoch = era * 146097 + day_of_era - 719468;
    let total_seconds = days_since_epoch * 86_400 + hour * 3_600 + minute * 60 + second;

    (total_seconds >= 0).then_some((total_seconds as u64) * 1000)
}

pub(crate) fn broadcast_runtime_events(
    events: Vec<RuntimeEvent>,
    broadcast_tx: &broadcast::Sender<SsePayload>,
) -> Option<TurnLifecycle> {
    let mut turn = None;

    for event in events {
        match event {
            RuntimeEvent::TurnLifecycle { turn: lifecycle } => {
                turn = Some(lifecycle);
            }
            RuntimeEvent::ContextCompressed { summary } => {
                let _ = broadcast_tx.send(SsePayload::ContextCompressed { summary });
            }
            RuntimeEvent::UserMessage { .. }
            | RuntimeEvent::AssistantMessage { .. }
            | RuntimeEvent::ToolInvocation { .. }
            | RuntimeEvent::TurnFailed { .. } => {}
        }
    }

    turn
}

#[derive(Clone, PartialEq)]
enum CurrentStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
}

impl CurrentStatus {
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
    use super::*;

    use agent_core::{ModelDisposition, ModelIdentity};
    use builtin_tools::build_tool_registry;
    use llm_trace::{LlmTraceStore, SqliteLlmTraceStore};
    use provider_registry::{ModelLimit, ProviderProfile};

    fn provider(name: &str, model: &str) -> ProviderProfile {
        ProviderProfile {
            name: name.to_string(),
            kind: ProviderKind::OpenAiResponses,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: model.to_string(),
                display_name: None,
                limit: Some(ModelLimit { context: Some(200_000), output: Some(131_072) }),
                default_temperature: None,
                supports_reasoning: false,
                reasoning_effort: None,
            }],
            active_model: Some(model.to_string()),
        }
    }

    fn owner_state(
        session_path: &str,
        store_path: &str,
        registry: ProviderRegistry,
    ) -> RuntimeOwnerState {
        let runtime = AgentRuntime::new(
            crate::model::ServerModel::bootstrap(),
            build_tool_registry(),
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
        );
        let (broadcast_tx, _) = broadcast::channel(16);
        let trace_store: Arc<dyn LlmTraceStore> =
            Arc::new(SqliteLlmTraceStore::in_memory().expect("trace store should init"));
        RuntimeOwnerState {
            runtime,
            subscriber: 1,
            session_path: PathBuf::from(session_path),
            registry,
            store_path: PathBuf::from(store_path),
            trace_store,
            broadcast_tx,
            provider_registry_snapshot: Arc::new(RwLock::new(ProviderRegistry::default())),
            provider_info_snapshot: Arc::new(RwLock::new(ProviderInfoSnapshot {
                name: "local".to_string(),
                model: "bootstrap".to_string(),
                connected: true,
            })),
            history_snapshot: Arc::new(RwLock::new(Vec::new())),
            current_turn_snapshot: Arc::new(RwLock::new(None)),
        }
    }

    #[test]
    fn sync_runtime_to_active_provider_tracks_registry_after_active_delete() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("first", "gpt-4.1-mini"));
        registry.upsert(provider("second", "gpt-4.1"));
        registry.set_active("first").expect("first provider should exist");

        let mut owner = owner_state("/tmp/session.jsonl", "/tmp/providers.json", registry);
        owner.registry.remove("first").expect("active provider should be removable");
        let candidate_registry = owner.registry.clone();

        let info = sync_runtime_to_registry(&mut owner, candidate_registry)
            .expect("runtime sync should follow the new active provider");

        assert_eq!(info.name, "openai");
        assert_eq!(info.model, "gpt-4.1");
        assert_eq!(
            owner.runtime.tape().latest_provider_binding(),
            Some(SessionProviderBinding::Provider {
                name: "second".to_string(),
                model: "gpt-4.1".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                protocol: "openai-responses".to_string(),
            })
        );
    }

    #[test]
    fn sync_runtime_to_active_provider_falls_back_to_bootstrap_when_empty() {
        let mut owner =
            owner_state("/tmp/session.jsonl", "/tmp/providers.json", ProviderRegistry::default());
        let candidate_registry = owner.registry.clone();

        let info = sync_runtime_to_registry(&mut owner, candidate_registry)
            .expect("runtime sync should support bootstrap fallback");

        assert_eq!(info.name, "local");
        assert_eq!(info.model, "bootstrap");
        assert_eq!(
            owner.runtime.tape().latest_provider_binding(),
            Some(SessionProviderBinding::Bootstrap)
        );
    }

    #[test]
    fn sync_runtime_to_registry_failure_does_not_mutate_when_registry_save_fails() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("stable", "gpt-4.1-mini"));
        let mut owner = owner_state(
            "/tmp/session.jsonl",
            "/proc/aia/providers.json",
            ProviderRegistry::default(),
        );

        let before_identity = owner.runtime.model_identity().clone();
        let before_binding = owner.runtime.tape().latest_provider_binding();

        let error =
            sync_runtime_to_registry(&mut owner, registry).expect_err("registry save should fail");

        assert!(error.message.contains("provider registry save failed"));
        assert_eq!(owner.runtime.model_identity(), &before_identity);
        assert_eq!(owner.runtime.tape().latest_provider_binding(), before_binding);
        assert!(owner.registry.providers().is_empty());
    }

    #[test]
    fn sync_runtime_to_registry_failure_does_not_mutate_when_session_save_fails() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("stable", "gpt-4.1-mini"));
        let mut owner = owner_state(
            "/proc/aia/session.jsonl",
            "/tmp/providers.json",
            ProviderRegistry::default(),
        );

        let before_identity = owner.runtime.model_identity().clone();
        let before_binding = owner.runtime.tape().latest_provider_binding();

        let error =
            sync_runtime_to_registry(&mut owner, registry).expect_err("session save should fail");

        assert!(error.message.contains("session save failed"));
        assert_eq!(owner.runtime.model_identity(), &before_identity);
        assert_eq!(owner.runtime.tape().latest_provider_binding(), before_binding);
        assert!(owner.registry.providers().is_empty());
    }

    #[test]
    fn switch_provider_refreshes_snapshots() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider("first", "gpt-4.1-mini"));
        registry.upsert(provider("second", "gpt-4.1"));
        registry.set_active("first").expect("first provider should exist");
        let mut owner = owner_state("/tmp/session.jsonl", "/tmp/providers.json", registry);
        let candidate_registry = owner.registry.clone();
        sync_runtime_to_registry(&mut owner, candidate_registry)
            .expect("initial sync should succeed");

        let info = switch_provider(
            &mut owner,
            SwitchProviderInput { name: "second".to_string(), model_id: None },
        )
        .expect("switch should succeed");

        let registry_snapshot = owner.provider_registry_snapshot.read().expect("lock poisoned");
        let info_snapshot = owner.provider_info_snapshot.read().expect("lock poisoned");

        assert_eq!(info.name, "openai");
        assert_eq!(owner.registry.active_provider().map(|p| p.name.as_str()), Some("second"));
        assert_eq!(registry_snapshot.active_provider().map(|p| p.name.as_str()), Some("second"));
        assert_eq!(info_snapshot.model, "gpt-4.1");
    }
}
