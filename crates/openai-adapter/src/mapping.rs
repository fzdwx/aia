use agent_core::{CompletionRequest, ConversationItem, Role};
use serde_json::{Value, json};

use crate::OpenAiAdapterError;

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

pub(crate) fn responses_continuation(request: &CompletionRequest) -> Option<(String, Vec<Value>)> {
    if let Some(checkpoint) = request.resume_checkpoint.as_ref() {
        if checkpoint.protocol == "openai-responses" {
            let input = request
                .conversation
                .iter()
                .filter_map(|item| match item {
                    ConversationItem::ToolResult(_) | ConversationItem::Message(_) => {
                        Some(responses_input_item(item))
                    }
                    ConversationItem::ToolCall(_) => None,
                })
                .collect::<Vec<_>>();
            if !input.is_empty() {
                return Some((checkpoint.token.clone(), input));
            }
        }
    }

    let latest_response_id = request.conversation.iter().rev().find_map(|item| match item {
        ConversationItem::ToolResult(result) => result.response_id.clone(),
        ConversationItem::Message(_) | ConversationItem::ToolCall(_) => None,
    })?;

    let last_user_index = request
        .conversation
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| {
            item.as_message().filter(|message| message.role == Role::User).map(|_| index)
        })
        .unwrap_or(0);

    let input = request
        .conversation
        .iter()
        .skip(last_user_index + 1)
        .filter_map(|item| match item {
            ConversationItem::ToolResult(result)
                if result.response_id.as_deref() == Some(latest_response_id.as_str()) =>
            {
                Some(responses_input_item(item))
            }
            ConversationItem::Message(_)
            | ConversationItem::ToolCall(_)
            | ConversationItem::ToolResult(_) => None,
        })
        .collect::<Vec<_>>();

    if input.is_empty() { None } else { Some((latest_response_id, input)) }
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
