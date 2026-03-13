use std::collections::BTreeMap;

use agent_core::Role;
use agent_runtime::{ToolInvocationLifecycle, ToolInvocationOutcome, TurnBlock, TurnLifecycle};
use session_tape::{SessionTape, TapeEntry};

fn find_failure_message(entries: &[&TapeEntry], call_id: u64) -> String {
    entries
        .iter()
        .filter(|entry| entry.kind == "event")
        .find_map(|entry| {
            let ids = entry.meta.get("source_entry_ids").and_then(|value| value.as_array())?;
            if ids.iter().any(|value| value.as_u64() == Some(call_id)) {
                entry
                    .event_data()
                    .and_then(|data| data.get("message"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown failure".into())
}

fn find_tool_result<'a>(
    entries: &[&'a TapeEntry],
    invocation_id: &str,
) -> Option<agent_core::ToolResult> {
    entries.iter().find_map(|entry| {
        let result = entry.as_tool_result()?;
        if result.invocation_id == invocation_id { Some(result) } else { None }
    })
}

pub(crate) fn reconstruct_turns(tape: &SessionTape) -> Vec<TurnLifecycle> {
    let mut groups: BTreeMap<String, Vec<&TapeEntry>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for entry in tape.entries() {
        if let Some(run_id) = entry.meta.get("run_id").and_then(|value| value.as_str()) {
            let run_id = run_id.to_string();
            if !groups.contains_key(&run_id) {
                order.push(run_id.clone());
            }
            groups.entry(run_id).or_default().push(entry);
        }
    }

    order
        .into_iter()
        .filter_map(|run_id| {
            let entries = groups.remove(&run_id)?;
            let user_message = entries
                .iter()
                .find_map(|entry| entry.as_message().filter(|message| message.role == Role::User))
                .map(|message| message.content)?;
            let calls: Vec<_> = entries
                .iter()
                .filter_map(|entry| entry.as_tool_call().map(|call| (entry.id, call)))
                .collect();
            let mut tool_invocations = Vec::new();
            for (call_id, call) in &calls {
                let failure_message = find_failure_message(&entries, *call_id);
                let outcome = if failure_message != "unknown failure" {
                    ToolInvocationOutcome::Failed { message: failure_message }
                } else if let Some(result) = find_tool_result(&entries, &call.invocation_id) {
                    ToolInvocationOutcome::Succeeded { result }
                } else {
                    ToolInvocationOutcome::Failed { message: "unknown failure".into() }
                };
                tool_invocations.push(ToolInvocationLifecycle { call: call.clone(), outcome });
            }

            let mut blocks = Vec::new();
            for entry in &entries {
                if let Some(thinking) = entry.as_thinking() {
                    blocks.push(TurnBlock::Thinking { content: thinking.to_string() });
                    continue;
                }
                if let Some(message) = entry.as_message() {
                    if message.role == Role::Assistant {
                        blocks.push(TurnBlock::Assistant { content: message.content.clone() });
                    }
                    continue;
                }
                if let Some(call) = entry.as_tool_call() {
                    if let Some(invocation) = tool_invocations
                        .iter()
                        .find(|invocation| invocation.call.invocation_id == call.invocation_id)
                    {
                        blocks.push(TurnBlock::ToolInvocation { invocation: invocation.clone() });
                    }
                    continue;
                }
                if entry.kind == "event" && entry.event_name() == Some("turn_failed") {
                    if let Some(message) = entry
                        .event_data()
                        .and_then(|data| data.get("message"))
                        .and_then(|value| value.as_str())
                    {
                        blocks.push(TurnBlock::Failure { message: message.to_string() });
                    }
                }
            }

            let assistant_message = blocks.iter().rev().find_map(|block| match block {
                TurnBlock::Assistant { content } => Some(content.clone()),
                TurnBlock::Thinking { .. }
                | TurnBlock::ToolInvocation { .. }
                | TurnBlock::Failure { .. } => None,
            });
            let thinking_parts = blocks
                .iter()
                .filter_map(|block| match block {
                    TurnBlock::Thinking { content } => Some(content.as_str()),
                    TurnBlock::Assistant { .. }
                    | TurnBlock::ToolInvocation { .. }
                    | TurnBlock::Failure { .. } => None,
                })
                .collect::<Vec<_>>();
            let thinking =
                if thinking_parts.is_empty() { None } else { Some(thinking_parts.join("")) };

            let failure_message = entries.iter().find_map(|entry| {
                if entry.kind == "error" {
                    entry
                        .payload
                        .get("message")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                } else if entry.kind == "event" && entry.event_name() == Some("turn_failed") {
                    entry
                        .event_data()
                        .and_then(|data| data.get("message"))
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                } else {
                    None
                }
            });

            Some(TurnLifecycle {
                turn_id: run_id,
                started_at_ms: 0,
                finished_at_ms: 0,
                source_entry_ids: entries.iter().map(|entry| entry.id).collect(),
                user_message,
                blocks,
                assistant_message,
                thinking,
                tool_invocations,
                failure_message,
            })
        })
        .collect()
}
