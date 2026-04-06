use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{Message, ToolCall, ToolResult, WidgetClientEvent, WidgetHostCommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TapeEntry {
    pub id: u64,
    pub kind: String,
    pub payload: Value,
    #[serde(default = "default_meta")]
    pub meta: Value,
    #[serde(default)]
    pub date: String,
}

pub(crate) fn default_meta() -> Value {
    Value::Object(serde_json::Map::new())
}

pub(crate) fn fallback_serialization_payload(kind: &str, error: &serde_json::Error) -> Value {
    serde_json::json!({
        "error": format!("failed to serialize {kind}: {error}")
    })
}

pub(crate) fn serialize_payload<T: Serialize>(kind: &str, value: &T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|error| fallback_serialization_payload(kind, &error))
}

impl TapeEntry {
    fn new(kind: &str, payload: Value) -> Self {
        Self { id: 0, kind: kind.to_string(), payload, meta: default_meta(), date: now_iso8601() }
    }

    pub fn message(msg: &Message) -> Self {
        Self::new("message", serialize_payload("message", msg))
    }

    pub fn system(content: &str) -> Self {
        Self::new("system", serde_json::json!({"content": content}))
    }

    pub fn anchor(name: &str, state: Option<Value>) -> Self {
        Self::new(
            "anchor",
            serde_json::json!({
                "name": name,
                "state": state.unwrap_or(Value::Object(serde_json::Map::new()))
            }),
        )
    }

    pub fn tool_call(call: &ToolCall) -> Self {
        Self::new("tool_call", serialize_payload("tool_call", call))
    }

    pub fn tool_result(result: &ToolResult) -> Self {
        Self::new("tool_result", serialize_payload("tool_result", result))
    }

    pub fn event(name: &str, data: Option<Value>) -> Self {
        Self::new(
            "event",
            serde_json::json!({
                "name": name,
                "data": data.unwrap_or(Value::Null)
            }),
        )
    }

    pub fn widget_host_command(invocation_id: &str, command: &WidgetHostCommand) -> Self {
        Self::event(
            "widget_host_command",
            Some(serde_json::json!({
                "invocation_id": invocation_id,
                "command": command,
            })),
        )
    }

    pub fn widget_client_event(invocation_id: &str, event: &WidgetClientEvent) -> Self {
        Self::event(
            "widget_client_event",
            Some(serde_json::json!({
                "invocation_id": invocation_id,
                "event": event,
            })),
        )
    }

    pub fn error(message: &str) -> Self {
        Self::new("error", serde_json::json!({"message": message}))
    }

    pub fn thinking(content: &str) -> Self {
        Self::new("thinking", serde_json::json!({"content": content}))
    }

    pub fn with_meta(mut self, key: &str, value: Value) -> Self {
        if let Value::Object(ref mut map) = self.meta {
            map.insert(key.to_string(), value);
        }
        self
    }

    pub fn with_run_id(self, run_id: &str) -> Self {
        self.with_meta("run_id", Value::String(run_id.to_string()))
    }

    pub fn as_message(&self) -> Option<Message> {
        if self.kind == "message" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    pub fn as_tool_call(&self) -> Option<ToolCall> {
        if self.kind == "tool_call" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    pub fn as_tool_result(&self) -> Option<ToolResult> {
        if self.kind == "tool_result" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    pub fn anchor_name(&self) -> Option<&str> {
        if self.kind == "anchor" {
            self.payload.get("name").and_then(|value| value.as_str())
        } else {
            None
        }
    }

    pub fn anchor_state(&self) -> Option<&Value> {
        if self.kind == "anchor" { self.payload.get("state") } else { None }
    }

    pub fn event_name(&self) -> Option<&str> {
        if self.kind == "event" {
            self.payload.get("name").and_then(|value| value.as_str())
        } else {
            None
        }
    }

    pub fn event_data(&self) -> Option<&Value> {
        if self.kind == "event" { self.payload.get("data") } else { None }
    }

    pub fn as_thinking(&self) -> Option<&str> {
        if self.kind == "thinking" {
            self.payload.get("content").and_then(|value| value.as_str())
        } else {
            None
        }
    }

    pub(crate) fn matches_text(&self, pattern: &str) -> bool {
        let lowered = pattern.to_lowercase();
        let haystack = self.payload.to_string().to_lowercase();
        if haystack.contains(&lowered) {
            return true;
        }
        self.kind.to_lowercase().contains(&lowered)
    }
}

pub(crate) fn now_iso8601() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let day_secs = (secs % 86400) as u32;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;
    let sec = day_secs % 60;

    let z = (secs / 86400) as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{mins:02}:{sec:02}Z")
}
