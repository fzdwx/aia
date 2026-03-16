use std::collections::BTreeMap;

use agent_core::{CompletionUsage, Role};
use agent_runtime::{TurnControl, TurnLifecycle, TurnOutcome};
use axum::http::StatusCode;
use provider_registry::{ModelConfig, ProviderKind};
use serde::{Deserialize, Serialize};
use session_tape::SessionTape;

use crate::sse::TurnStatus;

// ── Shared types ───────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentToolOutput {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub detected_at_ms: u64,
    pub started_at_ms: Option<u64>,
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
    pub fn from_identity(identity: &agent_core::ModelIdentity) -> Self {
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

#[derive(Clone)]
pub struct RunningTurnHandle {
    pub control: TurnControl,
}

// ── Tape snapshot reconstruction ───────────────────────────────

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

fn object_value(value: &serde_json::Value) -> serde_json::Value {
    if value.is_object() { value.clone() } else { serde_json::json!({}) }
}

// ── Turn history builder ───────────────────────────────────────

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
            self.failure_message = Some(message.clone());
            self.blocks.push(agent_runtime::TurnBlock::Failure { message });
            self.completed = true;
            return;
        }

        if entry.kind == "event" && entry.event_name() == Some("turn_completed") {
            self.usage = entry
                .event_data()
                .and_then(|value| value.get("usage"))
                .and_then(|value| serde_json::from_value(value.clone()).ok());
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
        let status = match lifecycle.outcome {
            TurnOutcome::Cancelled => TurnStatus::Cancelled,
            _ if lifecycle
                .blocks
                .iter()
                .any(|block| matches!(block, agent_runtime::TurnBlock::ToolInvocation { .. })) =>
            {
                TurnStatus::Working
            }
            _ if lifecycle
                .blocks
                .iter()
                .any(|block| matches!(block, agent_runtime::TurnBlock::Assistant { .. })) => {
                TurnStatus::Generating
            }
            _ if lifecycle
                .blocks
                .iter()
                .any(|block| matches!(block, agent_runtime::TurnBlock::Thinking { .. })) => {
                TurnStatus::Thinking
            }
            _ => TurnStatus::Waiting,
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
                        let invocation = *invocation;
                        let (result_content, result_details, failed) = match invocation.outcome {
                            agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                                (Some(result.content), result.details, Some(false))
                            }
                            agent_runtime::ToolInvocationOutcome::Failed { message } => {
                                (Some(message), None, Some(true))
                            }
                        };

                        Some(CurrentTurnBlock::Tool {
                            tool: CurrentToolOutput {
                                invocation_id: invocation.call.invocation_id,
                                tool_name: invocation.call.tool_name,
                                arguments: object_value(&invocation.call.arguments),
                                detected_at_ms: invocation.started_at_ms,
                                started_at_ms: Some(invocation.started_at_ms),
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

#[cfg(test)]
mod tests {
    use super::*;

    use agent_core::{Message, Role, ToolCall, ToolResult};
    use session_tape::TapeEntry;

    #[test]
    fn rebuild_turn_history_from_tape_restores_completed_turns() {
        let mut tape = SessionTape::new();
        let turn_id = "turn-1";
        let user = Message::new(Role::User, "你好");
        let assistant = Message::new(Role::Assistant, "已完成");
        let call = ToolCall::new("read").with_invocation_id("call-1");
        let result = ToolResult::from_call(&call, "内容");

        tape.append_entry(TapeEntry::message(&user).with_run_id(turn_id));
        tape.append_entry(TapeEntry::thinking("思考中").with_run_id(turn_id));
        tape.append_entry(TapeEntry::tool_call(&call).with_run_id(turn_id));
        tape.append_entry(TapeEntry::tool_result(&result).with_run_id(turn_id));
        tape.append_entry(TapeEntry::message(&assistant).with_run_id(turn_id));
        tape.append_entry(TapeEntry::event("turn_completed", None).with_run_id(turn_id));

        let turns = rebuild_session_snapshots_from_tape(&tape).history;

        assert_eq!(turns.len(), 1);
        let turn = &turns[0];
        assert_eq!(turn.turn_id, turn_id);
        assert_eq!(turn.user_message, "你好");
        assert_eq!(turn.assistant_message.as_deref(), Some("已完成"));
        assert_eq!(turn.thinking.as_deref(), Some("思考中"));
        assert_eq!(turn.tool_invocations.len(), 1);
        assert_eq!(turn.blocks.len(), 3);
    }

    #[test]
    fn rebuild_turn_history_from_tape_restores_turn_usage() {
        let mut tape = SessionTape::new();
        let turn_id = "turn-usage";
        tape.append_entry(
            TapeEntry::message(&Message::new(Role::User, "统计 token")).with_run_id(turn_id),
        );
        tape.append_entry(
            TapeEntry::message(&Message::new(Role::Assistant, "本次调用已完成"))
                .with_run_id(turn_id),
        );
        tape.append_entry(
            TapeEntry::event(
                "turn_completed",
                Some(serde_json::json!({
                    "status": "ok",
                    "usage": {
                        "input_tokens": 21,
                        "output_tokens": 9,
                        "total_tokens": 30,
                        "cached_tokens": 0
                    }
                })),
            )
            .with_run_id(turn_id),
        );

        let turns = rebuild_session_snapshots_from_tape(&tape).history;

        assert_eq!(turns.len(), 1);
        assert_eq!(
            turns[0].usage,
            Some(CompletionUsage {
                input_tokens: 21,
                output_tokens: 9,
                total_tokens: 30,
                cached_tokens: 0,
            })
        );
    }

    #[test]
    fn rebuild_session_snapshots_from_tape_keeps_incomplete_turn_out_of_history() {
        let mut tape = SessionTape::new();
        let turn_id = "turn-1";
        tape.append_entry(
            TapeEntry::message(&Message::new(Role::User, "处理中")).with_run_id(turn_id),
        );
        tape.append_entry(TapeEntry::thinking("先分析").with_run_id(turn_id));

        let snapshots = rebuild_session_snapshots_from_tape(&tape);

        assert!(snapshots.history.is_empty());
        let current = snapshots.current_turn.expect("应保留当前未完成轮次");
        assert_eq!(current.user_message, "处理中");
        assert_eq!(current.status, crate::sse::TurnStatus::Thinking);
        assert!(current.started_at_ms > 0);
        assert_eq!(
            current.blocks,
            vec![CurrentTurnBlock::Thinking { content: "先分析".to_string() }]
        );
    }

    #[test]
    fn rebuild_turn_history_from_tape_restores_legacy_turn_record() {
        let mut tape = SessionTape::new();
        let legacy_turn = TurnLifecycle {
            turn_id: "legacy-turn-1".to_string(),
            started_at_ms: 1000,
            finished_at_ms: 2000,
            source_entry_ids: vec![1, 2],
            user_message: "旧问题".to_string(),
            blocks: vec![agent_runtime::TurnBlock::Assistant { content: "旧回答".to_string() }],
            assistant_message: Some("旧回答".to_string()),
            thinking: None,
            tool_invocations: vec![],
            usage: None,
            failure_message: None,
            outcome: TurnOutcome::Succeeded,
        };
        tape.append_entry(TapeEntry::event(
            "turn_record",
            Some(serde_json::to_value(&legacy_turn).expect("legacy turn should serialize")),
        ));

        let turns = rebuild_session_snapshots_from_tape(&tape).history;

        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0], legacy_turn);
    }
}
