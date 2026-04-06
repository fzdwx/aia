use std::collections::BTreeMap;

use agent_core::{CompletionUsage, Role, StreamEvent, WidgetHostCommand};
use agent_runtime::{ToolInvocationReplayEvent, TurnLifecycle, TurnOutcome};
use session_tape::SessionTape;

use super::{
    CurrentTurnBlock, CurrentTurnSnapshot, find_tool_output_mut, live_tool_block,
    normalize_object_value, sync_widget_projection,
};

#[derive(Default)]
pub struct SessionSnapshots {
    pub history: Vec<TurnLifecycle>,
    pub current_turn: Option<CurrentTurnSnapshot>,
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

fn parse_legacy_turn_record(entry: &session_tape::TapeEntry) -> Option<TurnLifecycle> {
    let data = entry.event_data()?.clone();
    match serde_json::from_value(data) {
        Ok(turn) => Some(turn),
        Err(error) => {
            eprintln!("legacy turn_record decode failed for entry {}: {error}", entry.id);
            None
        }
    }
}

fn decode_completion_usage(
    entry: &session_tape::TapeEntry,
    usage: &serde_json::Value,
) -> Option<CompletionUsage> {
    match serde_json::from_value(usage.clone()) {
        Ok(usage) => Some(usage),
        Err(error) => {
            eprintln!("turn_completed usage decode failed for entry {}: {error}", entry.id);
            None
        }
    }
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

#[derive(Default)]
struct TurnHistoryBuilder {
    turn_id: String,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    source_entry_ids: Vec<u64>,
    user_messages: Vec<String>,
    blocks: Vec<agent_runtime::TurnBlock>,
    assistant_message: Option<String>,
    thinking: Option<String>,
    tool_invocations: Vec<agent_runtime::ToolInvocationLifecycle>,
    usage: Option<CompletionUsage>,
    failure_message: Option<String>,
    pending_tool_calls: BTreeMap<String, (agent_core::ToolCall, u64)>,
    replay_events_by_invocation: BTreeMap<String, Vec<ToolInvocationReplayEvent>>,
    current_turn: Option<CurrentTurnSnapshot>,
    completed: bool,
    waiting_for_question: bool,
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

        self.ensure_current_turn(timestamp_ms);

        if let Some(message) = entry.as_message() {
            match message.role {
                Role::User => {
                    let content = message.content.clone();
                    self.user_messages.push(content.clone());
                    if let Some(current) = self.current_turn.as_mut() {
                        current.user_messages.push(content);
                    }
                }
                Role::Assistant => {
                    let content = message.content.clone();
                    self.assistant_message = Some(content.clone());
                    self.blocks
                        .push(agent_runtime::TurnBlock::Assistant { content: content.clone() });
                    if let Some(current) = self.current_turn.as_mut() {
                        current.blocks.push(CurrentTurnBlock::Text { content });
                    }
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
            if let Some(current) = self.current_turn.as_mut() {
                current.blocks.push(CurrentTurnBlock::Thinking { content: content.to_string() });
            }
            return;
        }

        if let Some(call) = entry.as_tool_call() {
            self.pending_tool_calls
                .insert(call.invocation_id.clone(), (call.clone(), timestamp_ms));
            self.sync_current_tool_call(Some(&(call, timestamp_ms)));
            return;
        }

        if let Some(result) = entry.as_tool_result() {
            let (call, started_at_ms) =
                self.pending_tool_calls.remove(&result.invocation_id).unwrap_or_else(|| {
                    (
                        agent_core::ToolCall::new(result.tool_name.clone())
                            .with_invocation_id(result.invocation_id.clone()),
                        timestamp_ms,
                    )
                });
            let invocation = agent_runtime::ToolInvocationLifecycle {
                call,
                started_at_ms,
                finished_at_ms: timestamp_ms,
                trace_context: None,
                replay_events: self
                    .replay_events_by_invocation
                    .remove(&result.invocation_id)
                    .unwrap_or_default(),
                outcome: agent_runtime::ToolInvocationOutcome::Succeeded { result },
            };
            self.sync_current_tool_result(&invocation, timestamp_ms);
            self.blocks.push(agent_runtime::TurnBlock::ToolInvocation {
                invocation: Box::new(invocation.clone()),
            });
            self.tool_invocations.push(invocation);
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("widget_host_command") {
            if let Some(payload) =
                entry.event_data().cloned().and_then(parse_widget_host_command_event)
            {
                self.append_replay_event(
                    &payload.invocation_id,
                    ToolInvocationReplayEvent::WidgetHostCommand {
                        command: payload.command.clone(),
                    },
                );
                self.apply_current_stream_event(
                    &StreamEvent::WidgetHostCommand {
                        invocation_id: payload.invocation_id,
                        command: payload.command,
                    },
                    timestamp_ms,
                );
            }
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("widget_client_event") {
            if let Some(payload) = entry.event_data().cloned().and_then(parse_widget_client_event) {
                self.append_replay_event(
                    &payload.invocation_id,
                    ToolInvocationReplayEvent::WidgetClientEvent { event: payload.event },
                );
            }
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_failed") {
            let message = entry
                .event_data()
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .unwrap_or("turn failed")
                .to_string();
            let cancelled = message.contains("已取消");
            self.failure_message = Some(message.clone());
            self.blocks.push(if cancelled {
                agent_runtime::TurnBlock::Cancelled { message }
            } else {
                agent_runtime::TurnBlock::Failure { message }
            });
            self.completed = true;
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_completed") {
            self.usage = entry
                .event_data()
                .and_then(|value| value.get("usage"))
                .and_then(|value| decode_completion_usage(entry, value));
            self.completed = true;
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_waiting_for_question") {
            self.completed = true;
            self.waiting_for_question = true;
        }
    }

    fn into_turn_lifecycle(self) -> Option<TurnLifecycle> {
        if self.user_messages.is_empty() {
            return None;
        }
        Some(TurnLifecycle {
            turn_id: self.turn_id,
            started_at_ms: self.started_at_ms.unwrap_or(0),
            finished_at_ms: self.finished_at_ms.unwrap_or(0),
            source_entry_ids: self.source_entry_ids,
            user_messages: self.user_messages,
            blocks: self.blocks,
            assistant_message: self.assistant_message,
            thinking: self.thinking,
            tool_invocations: self.tool_invocations,
            usage: self.usage,
            failure_message: self.failure_message.clone(),
            outcome: if self
                .failure_message
                .as_deref()
                .is_some_and(|message| message.contains("已取消"))
            {
                TurnOutcome::Cancelled
            } else if self.waiting_for_question {
                TurnOutcome::WaitingForQuestion
            } else if self.failure_message.is_some() {
                TurnOutcome::Failed
            } else {
                TurnOutcome::Succeeded
            },
        })
    }

    fn is_completed(&self) -> bool {
        self.completed
    }

    fn into_current_turn(self) -> Option<CurrentTurnSnapshot> {
        let mut current = self.current_turn?;
        if current.user_messages.is_empty() {
            return None;
        }

        current.status = if self.waiting_for_question {
            crate::sse::TurnStatus::WaitingForQuestion
        } else if current.blocks.iter().any(|block| matches!(block, CurrentTurnBlock::Tool { .. }))
        {
            crate::sse::TurnStatus::Working
        } else if current.blocks.iter().any(|block| matches!(block, CurrentTurnBlock::Text { .. }))
        {
            crate::sse::TurnStatus::Generating
        } else if current
            .blocks
            .iter()
            .any(|block| matches!(block, CurrentTurnBlock::Thinking { .. }))
        {
            crate::sse::TurnStatus::Thinking
        } else {
            crate::sse::TurnStatus::Waiting
        };

        Some(current)
    }

    fn ensure_current_turn(&mut self, timestamp_ms: u64) {
        self.current_turn.get_or_insert_with(|| CurrentTurnSnapshot {
            turn_id: self.turn_id.clone(),
            started_at_ms: self.started_at_ms.unwrap_or(timestamp_ms),
            user_messages: Vec::new(),
            status: crate::sse::TurnStatus::Waiting,
            blocks: Vec::new(),
        });
    }

    fn sync_current_tool_call(&mut self, call: Option<&(agent_core::ToolCall, u64)>) {
        let Some((call, started_at_ms)) = call else {
            return;
        };
        let Some(current) = self.current_turn.as_mut() else {
            return;
        };

        if let Some(tool) = find_tool_output_mut(&mut current.blocks, &call.invocation_id) {
            tool.tool_name = call.tool_name.clone();
            tool.arguments = normalize_object_value(&call.arguments);
            tool.raw_arguments =
                serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
            tool.detected_at_ms = *started_at_ms;
            tool.started_at_ms = Some(*started_at_ms);
            sync_widget_projection(tool);
            return;
        }

        let mut block = live_tool_block(
            call.invocation_id.clone(),
            call.tool_name.clone(),
            normalize_object_value(&call.arguments),
            String::new(),
            None,
            *started_at_ms,
            true,
        );
        if let CurrentTurnBlock::Tool { tool } = &mut block {
            tool.raw_arguments =
                serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
            sync_widget_projection(tool);
        }
        current.blocks.push(block);
    }

    fn sync_current_tool_result(
        &mut self,
        invocation: &agent_runtime::ToolInvocationLifecycle,
        finished_at_ms: u64,
    ) {
        self.sync_current_tool_call(Some(&(invocation.call.clone(), invocation.started_at_ms)));
        let Some(current) = self.current_turn.as_mut() else {
            return;
        };
        let Some(tool) = find_tool_output_mut(&mut current.blocks, &invocation.call.invocation_id)
        else {
            return;
        };

        tool.completed = true;
        tool.finished_at_ms = Some(finished_at_ms);
        match &invocation.outcome {
            agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                tool.result_content = Some(result.content.clone());
                tool.result_details = result.details.clone();
                tool.failed = Some(false);
            }
            agent_runtime::ToolInvocationOutcome::Failed { message } => {
                tool.result_content = Some(message.clone());
                tool.result_details = None;
                tool.failed = Some(true);
            }
        }
        sync_widget_projection(tool);
    }

    fn apply_current_stream_event(&mut self, event: &StreamEvent, timestamp_ms: u64) {
        let Some(current) = self.current_turn.as_mut() else {
            return;
        };

        if let StreamEvent::WidgetHostCommand {
            invocation_id,
            command: WidgetHostCommand::Render { widget },
        } = event
        {
            if let Some(tool) = find_tool_output_mut(&mut current.blocks, invocation_id) {
                tool.widget = Some(widget.clone());
                return;
            }

            let mut block = live_tool_block(
                invocation_id.clone(),
                "WidgetRenderer".to_string(),
                serde_json::json!({}),
                String::new(),
                None,
                timestamp_ms,
                false,
            );
            if let CurrentTurnBlock::Tool { tool } = &mut block {
                tool.widget = Some(widget.clone());
            }
            current.blocks.push(block);
        }
    }

    fn append_replay_event(&mut self, invocation_id: &str, event: ToolInvocationReplayEvent) {
        if let Some(invocation) = self
            .tool_invocations
            .iter_mut()
            .find(|invocation| invocation.call.invocation_id == invocation_id)
        {
            invocation.replay_events.push(event.clone());
        } else {
            self.replay_events_by_invocation
                .entry(invocation_id.to_string())
                .or_default()
                .push(event.clone());
        }

        for block in &mut self.blocks {
            if let agent_runtime::TurnBlock::ToolInvocation { invocation } = block
                && invocation.call.invocation_id == invocation_id
            {
                invocation.replay_events.push(event.clone());
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct PersistedWidgetHostCommandEvent {
    invocation_id: String,
    command: WidgetHostCommand,
}

#[derive(serde::Deserialize)]
struct PersistedWidgetClientEvent {
    invocation_id: String,
    event: agent_core::WidgetClientEvent,
}

fn parse_widget_host_command_event(
    value: serde_json::Value,
) -> Option<PersistedWidgetHostCommandEvent> {
    serde_json::from_value(value).ok()
}

fn parse_widget_client_event(value: serde_json::Value) -> Option<PersistedWidgetClientEvent> {
    serde_json::from_value(value).ok()
}
