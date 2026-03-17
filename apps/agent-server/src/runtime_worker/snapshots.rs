use std::collections::BTreeMap;

use agent_core::{CompletionUsage, Role};
use agent_runtime::{TurnLifecycle, TurnOutcome};
use session_tape::SessionTape;

use super::{CurrentTurnSnapshot, turn_block_to_current, turn_lifecycle_status};

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
    user_message: Option<String>,
    blocks: Vec<agent_runtime::TurnBlock>,
    assistant_message: Option<String>,
    thinking: Option<String>,
    tool_invocations: Vec<agent_runtime::ToolInvocationLifecycle>,
    usage: Option<CompletionUsage>,
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
                trace_context: None,
                outcome: agent_runtime::ToolInvocationOutcome::Succeeded { result },
            };
            self.blocks.push(agent_runtime::TurnBlock::ToolInvocation {
                invocation: Box::new(invocation.clone()),
            });
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
            usage: self.usage,
            failure_message: self.failure_message.clone(),
            outcome: if self
                .failure_message
                .as_deref()
                .is_some_and(|message| message.contains("已取消"))
            {
                TurnOutcome::Cancelled
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
        let lifecycle = self.into_turn_lifecycle()?;
        let status = turn_lifecycle_status(&lifecycle);

        Some(CurrentTurnSnapshot {
            started_at_ms: lifecycle.started_at_ms,
            user_message: lifecycle.user_message,
            status,
            blocks: lifecycle.blocks.into_iter().filter_map(turn_block_to_current).collect(),
        })
    }
}
