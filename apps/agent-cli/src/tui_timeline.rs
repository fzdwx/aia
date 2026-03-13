use std::collections::BTreeMap;

use agent_core::Role;
use agent_runtime::{ToolInvocationLifecycle, ToolInvocationOutcome, TurnLifecycle};
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
            let assistant_message = entries
                .iter()
                .find_map(|entry| {
                    entry.as_message().filter(|message| message.role == Role::Assistant)
                })
                .map(|message| message.content);

            let thinking =
                entries.iter().find_map(|entry| entry.as_thinking().map(|value| value.to_string()));

            let calls: Vec<_> = entries
                .iter()
                .filter_map(|entry| entry.as_tool_call().map(|call| (entry.id, call)))
                .collect();
            let mut tool_invocations = Vec::new();
            for (call_id, call) in &calls {
                let outcome = entries
                    .iter()
                    .find_map(|entry| {
                        let result = entry.as_tool_result()?;
                        if result.invocation_id == call.invocation_id {
                            Some(ToolInvocationOutcome::Succeeded { result })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| ToolInvocationOutcome::Failed {
                        message: find_failure_message(&entries, *call_id),
                    });
                tool_invocations.push(ToolInvocationLifecycle { call: call.clone(), outcome });
            }

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
                assistant_message,
                thinking,
                tool_invocations,
                failure_message,
            })
        })
        .collect()
}
