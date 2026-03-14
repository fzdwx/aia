use agent_core::{ConversationItem, Message, Role};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::TapeEntry;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub entry_id: u64,
    pub name: String,
    pub state: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Handoff {
    pub anchor: Anchor,
    pub event_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    pub origin_anchor: Option<Anchor>,
    pub entries: Vec<TapeEntry>,
    pub messages: Vec<Message>,
    pub conversation: Vec<ConversationItem>,
}

pub(crate) fn anchor_from_entry(entry: &TapeEntry) -> Option<Anchor> {
    if entry.kind != "anchor" {
        return None;
    }
    let name = entry.anchor_name()?.to_string();
    let state = entry.anchor_state().cloned().unwrap_or(Value::Object(serde_json::Map::new()));
    Some(Anchor { entry_id: entry.id, name, state })
}

pub(crate) fn project_message(entry: &TapeEntry) -> Option<Message> {
    if entry.kind == "thinking" {
        return None;
    }

    if let Some(message) = entry.as_message() {
        return Some(message);
    }

    entry.as_tool_result().map(|result| {
        Message::new(
            Role::Tool,
            format!("工具 {} #{} 输出: {}", result.tool_name, result.invocation_id, result.content),
        )
    })
}

pub(crate) fn project_conversation_item(entry: &TapeEntry) -> Option<ConversationItem> {
    if entry.kind == "thinking" {
        return None;
    }

    if let Some(message) = entry.as_message() {
        return Some(ConversationItem::Message(message));
    }
    if let Some(call) = entry.as_tool_call() {
        return Some(ConversationItem::ToolCall(call));
    }
    entry.as_tool_result().map(ConversationItem::ToolResult)
}
