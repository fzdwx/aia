use std::collections::BTreeSet;

use agent_core::{ConversationItem, Role};
use serde_json::{Value, json};

use crate::OpenAiAdapterError;

#[derive(Default)]
pub(crate) struct StreamingToolCallAccumulator {
    invocation_id: Option<String>,
    tool_name: Option<String>,
    arguments: String,
    emitted_detection: bool,
    /// 已通过 ToolCallDetected 事件发送过的参数 key 集合
    emitted_arg_keys: BTreeSet<String>,
}

impl StreamingToolCallAccumulator {
    pub(crate) fn set_invocation_id(&mut self, invocation_id: impl Into<String>) {
        self.invocation_id = Some(invocation_id.into());
    }

    pub(crate) fn set_tool_name(&mut self, tool_name: impl Into<String>) {
        self.tool_name = Some(tool_name.into());
    }

    pub(crate) fn push_arguments_delta(&mut self, arguments_delta: &str) {
        self.arguments.push_str(arguments_delta);
    }

    pub(crate) fn invocation_id_or(&self, fallback: impl FnOnce() -> String) -> String {
        self.invocation_id.clone().unwrap_or_else(fallback)
    }

    pub(crate) fn tool_name(&self) -> Option<&str> {
        self.tool_name.as_deref()
    }

    pub(crate) fn parsed_arguments(&self) -> Value {
        parse_tool_arguments(&self.arguments).unwrap_or_default()
    }

    pub(crate) fn raw_arguments(&self) -> &str {
        self.arguments.as_str()
    }

    pub(crate) fn detection_emitted(&self) -> bool {
        self.emitted_detection
    }

    pub(crate) fn mark_detection_emitted(&mut self) {
        self.emitted_detection = true;
    }

    /// 检查当前累积的 arguments 是否解析出了新的完整参数 key，
    /// 如果有则更新内部跟踪并返回 true。
    pub(crate) fn check_new_parsed_keys(&mut self) -> bool {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&self.arguments) else {
            return false;
        };
        let current_keys: BTreeSet<String> = map.keys().cloned().collect();
        if current_keys.len() > self.emitted_arg_keys.len() {
            self.emitted_arg_keys = current_keys;
            return true;
        }
        false
    }

    pub(crate) fn clear(&mut self) {
        self.invocation_id = None;
        self.tool_name = None;
        self.arguments.clear();
        self.emitted_detection = false;
        self.emitted_arg_keys.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tool_name.is_none() && self.arguments.is_empty() && self.invocation_id.is_none()
    }
}

pub(crate) fn role_name(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

pub(crate) fn responses_input_item(item: &ConversationItem) -> Value {
    match item {
        ConversationItem::Message(message) => json!({
            "role": role_name(&message.role),
            "content": message.content,
        }),
        ConversationItem::ToolCall(call) => json!({
            "type": "function_call",
            "call_id": call.invocation_id,
            "name": call.tool_name,
            "arguments": serialize_tool_arguments(&call.arguments),
        }),
        ConversationItem::ToolResult(result) => json!({
            "type": "function_call_output",
            "call_id": result.invocation_id,
            "output": result.content,
        }),
    }
}

pub(crate) fn chat_completion_messages(conversation: &[ConversationItem]) -> Vec<Value> {
    let mut messages = Vec::new();

    for item in conversation {
        match item {
            ConversationItem::Message(message) => {
                messages.push(json!({
                    "role": role_name(&message.role),
                    "content": message.content,
                }));
            }
            ConversationItem::ToolCall(call) => {
                let tool_call = json!({
                    "id": call.invocation_id,
                    "type": "function",
                    "function": {
                        "name": call.tool_name,
                        "arguments": serialize_tool_arguments(&call.arguments),
                    }
                });

                if let Some(last) = messages.last_mut() {
                    let is_assistant =
                        last.get("role").and_then(|value| value.as_str()) == Some("assistant");
                    if is_assistant {
                        if last.get("content").is_none() {
                            last["content"] = json!("");
                        }
                        let mut tool_calls = last
                            .get("tool_calls")
                            .and_then(|value| value.as_array())
                            .cloned()
                            .unwrap_or_default();
                        tool_calls.push(tool_call);
                        last["tool_calls"] = Value::Array(tool_calls);
                        continue;
                    }
                }

                messages.push(json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [tool_call],
                }));
            }
            ConversationItem::ToolResult(result) => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": result.invocation_id,
                    "content": result.content,
                }));
            }
        }
    }

    messages
}

pub(crate) fn serialize_tool_arguments(arguments: &Value) -> String {
    serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())
}

pub(crate) fn parse_tool_arguments(arguments: &str) -> Result<Value, OpenAiAdapterError> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
    if !value.is_object() {
        return Err(OpenAiAdapterError::new("工具参数必须是对象"));
    }
    Ok(value)
}

pub(crate) fn extract_stream_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(values) => {
            let text = values.iter().filter_map(extract_stream_text).collect::<Vec<_>>().join("");
            if text.is_empty() { None } else { Some(text) }
        }
        Value::Object(map) => {
            for key in ["text", "summary_text", "content", "value"] {
                if let Some(text) = map.get(key).and_then(extract_stream_text) {
                    return Some(text);
                }
            }
            let text = map.values().filter_map(extract_stream_text).collect::<Vec<_>>().join("");
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

pub(crate) fn extract_reasoning_stream_text(event: &Value) -> Option<String> {
    extract_reasoning_summary_text(&event["delta"])
        .or_else(|| extract_reasoning_summary_text(&event["text"]))
        .or_else(|| extract_reasoning_summary_text(&event["part"]["text"]))
}

fn extract_reasoning_summary_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(values) => {
            let text = values
                .iter()
                .filter_map(extract_reasoning_summary_text)
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
        Value::Object(map) => {
            for key in ["text", "summary_text"] {
                if let Some(text) = map.get(key).and_then(extract_reasoning_summary_text) {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}
