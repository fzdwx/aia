use agent_core::{Message, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::entry::serialize_payload;
use crate::{SessionProviderBinding, SessionTapeError, TapeEntry, default_meta};

#[derive(Deserialize)]
struct LegacyEntry {
    id: u64,
    fact: LegacyFact,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Serialize, Deserialize)]
enum LegacyFact {
    Message(Message),
    SessionMetadata(LegacySessionMetadata),
    Anchor(LegacyAnchorState),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Turn(Value),
    Event(LegacySessionEvent),
    Error(LegacyErrorRecord),
}

#[derive(Serialize, Deserialize)]
struct LegacySessionMetadata {
    provider_binding: SessionProviderBinding,
}

#[derive(Serialize, Deserialize)]
struct LegacyAnchorState {
    #[serde(default = "default_anchor_name")]
    name: String,
    phase: String,
    summary: String,
    next_steps: Vec<String>,
    source_entry_ids: Vec<u64>,
    owner: String,
}

fn default_anchor_name() -> String {
    "anchor".into()
}

#[derive(Serialize, Deserialize)]
struct LegacySessionEvent {
    kind: String,
    detail: String,
    source_entry_ids: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
struct LegacyErrorRecord {
    message: String,
    #[serde(default)]
    source_entry_ids: Vec<u64>,
}

fn convert_legacy(legacy: LegacyEntry) -> TapeEntry {
    let date = legacy.date.unwrap_or_default();
    match legacy.fact {
        LegacyFact::Message(msg) => TapeEntry {
            id: legacy.id,
            kind: "message".into(),
            payload: serialize_payload("legacy_message", &msg),
            meta: default_meta(),
            date,
        },
        LegacyFact::SessionMetadata(meta) => TapeEntry {
            id: legacy.id,
            kind: "event".into(),
            payload: serde_json::json!({
                "name": "provider_binding",
                "data": serialize_payload("legacy_provider_binding", &meta.provider_binding)
            }),
            meta: default_meta(),
            date,
        },
        LegacyFact::Anchor(anchor) => TapeEntry {
            id: legacy.id,
            kind: "anchor".into(),
            payload: serde_json::json!({
                "name": anchor.name,
                "state": {
                    "phase": anchor.phase,
                    "summary": anchor.summary,
                    "next_steps": anchor.next_steps,
                    "source_entry_ids": anchor.source_entry_ids,
                    "owner": anchor.owner,
                }
            }),
            meta: default_meta(),
            date,
        },
        LegacyFact::ToolCall(call) => TapeEntry {
            id: legacy.id,
            kind: "tool_call".into(),
            payload: serialize_payload("legacy_tool_call", &call),
            meta: default_meta(),
            date,
        },
        LegacyFact::ToolResult(result) => TapeEntry {
            id: legacy.id,
            kind: "tool_result".into(),
            payload: serialize_payload("legacy_tool_result", &result),
            meta: default_meta(),
            date,
        },
        LegacyFact::Turn(value) => TapeEntry {
            id: legacy.id,
            kind: "event".into(),
            payload: serde_json::json!({"name": "turn_record", "data": value}),
            meta: default_meta(),
            date,
        },
        LegacyFact::Event(event) => TapeEntry {
            id: legacy.id,
            kind: "event".into(),
            payload: serde_json::json!({
                "name": event.kind,
                "data": {"detail": event.detail}
            }),
            meta: serde_json::json!({"source_entry_ids": event.source_entry_ids}),
            date,
        },
        LegacyFact::Error(error) => TapeEntry {
            id: legacy.id,
            kind: "error".into(),
            payload: serde_json::json!({"message": error.message}),
            meta: serde_json::json!({"source_entry_ids": error.source_entry_ids}),
            date,
        },
    }
}

pub(crate) fn decode_persisted_line(line: &str) -> Result<TapeEntry, SessionTapeError> {
    if let Ok(entry) = serde_json::from_str::<TapeEntry>(line)
        && !entry.kind.is_empty()
    {
        return Ok(entry);
    }
    if let Ok(legacy) = serde_json::from_str::<LegacyEntry>(line) {
        return Ok(convert_legacy(legacy));
    }
    Err(SessionTapeError::from_serde(serde_json::from_str::<Value>(line).unwrap_err()))
}
