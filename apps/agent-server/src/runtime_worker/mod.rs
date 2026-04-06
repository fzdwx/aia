mod snapshots;
#[cfg(test)]
#[path = "../../tests/runtime_worker/mod.rs"]
mod tests;

use agent_core::{ToolOutputStream, UiWidget, UiWidgetDocument, UiWidgetPhase};
use agent_runtime::TurnControl;
use axum::http::StatusCode;
use provider_registry::{ModelConfig, ProviderKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::sse::TurnStatus;

pub(crate) use snapshots::rebuild_session_snapshots_from_tape;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentToolOutputSegment {
    pub stream: ToolOutputStream,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentToolOutput {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub raw_arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<UiWidget>,
    pub detected_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_segments: Option<Vec<CurrentToolOutputSegment>>,
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
    pub turn_id: String,
    pub started_at_ms: u64,
    /// 用户消息列表，多条消息时有多个元素
    pub user_messages: Vec<String>,
    pub status: TurnStatus,
    pub blocks: Vec<CurrentTurnBlock>,
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

    pub fn queue_full(max_size: usize) -> Self {
        Self::bad_request(format!("message queue is full (max {} messages)", max_size))
    }

    pub fn message_not_found(id: &str) -> Self {
        Self::not_found(format!("message not found: {}", id))
    }

    pub fn cannot_modify_queue_while_running() -> Self {
        Self::bad_request("cannot modify message queue while session is running")
    }
}

#[derive(Clone)]
pub struct CreateProviderInput {
    pub name: String,
    pub kind: ProviderKind,
    pub models: Vec<ModelConfig>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct UpdateProviderInput {
    pub kind: Option<ProviderKind>,
    pub models: Option<Vec<ModelConfig>>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct RunningTurnHandle {
    pub control: TurnControl,
}

pub(crate) fn normalize_object_value(value: &Value) -> Value {
    if value.is_object() { value.clone() } else { json!({}) }
}

pub(crate) fn find_tool_output_mut<'a>(
    blocks: &'a mut [CurrentTurnBlock],
    invocation_id: &str,
) -> Option<&'a mut CurrentToolOutput> {
    blocks.iter_mut().rev().find_map(|block| match block {
        CurrentTurnBlock::Tool { tool } if tool.invocation_id == invocation_id => Some(tool),
        _ => None,
    })
}

pub(crate) fn live_tool_block(
    invocation_id: String,
    tool_name: String,
    arguments: Value,
    output: String,
    output_segments: Option<Vec<CurrentToolOutputSegment>>,
    timestamp_ms: u64,
    started: bool,
) -> CurrentTurnBlock {
    CurrentTurnBlock::Tool {
        tool: CurrentToolOutput {
            invocation_id,
            tool_name,
            arguments,
            raw_arguments: String::new(),
            widget: None,
            detected_at_ms: timestamp_ms,
            started_at_ms: started.then_some(timestamp_ms),
            finished_at_ms: None,
            output,
            output_segments,
            completed: false,
            result_content: None,
            result_details: None,
            failed: None,
        },
    }
}

fn is_widget_renderer_tool_name(tool_name: &str) -> bool {
    tool_name == "WidgetRenderer" || tool_name == "widgetRenderer"
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(ToString::to_string)
}

fn extract_widget_html_from_raw_arguments(raw_arguments: &str) -> Option<String> {
    let html_key_index = raw_arguments.find("\"html\"")?;
    let first_quote_index = raw_arguments[html_key_index + 6..].find('"')? + html_key_index + 6;

    let mut cursor = first_quote_index + 1;
    let mut escaped = false;
    let mut extracted = String::new();
    while cursor < raw_arguments.len() {
        let current = raw_arguments[cursor..].chars().next()?;
        if escaped {
            match current {
                'n' => extracted.push('\n'),
                'r' => extracted.push('\r'),
                't' => extracted.push('\t'),
                '"' | '\\' | '/' => extracted.push(current),
                _ => extracted.push(current),
            }
            escaped = false;
            cursor += current.len_utf8();
            continue;
        }

        if current == '\\' {
            escaped = true;
            cursor += current.len_utf8();
            continue;
        }
        if current == '"' {
            break;
        }
        extracted.push(current);
        cursor += current.len_utf8();
    }

    let trimmed = extracted.trim();
    (!trimmed.is_empty()).then_some(trimmed.to_string())
}

fn derive_widget_document(tool: &CurrentToolOutput) -> Option<UiWidgetDocument> {
    if !is_widget_renderer_tool_name(&tool.tool_name) {
        return None;
    }

    let details = tool.result_details.as_ref();
    let title = details
        .and_then(|value| string_field(value, "title"))
        .or_else(|| string_field(&tool.arguments, "title"))
        .unwrap_or_else(|| "Widget".to_string());
    let description = details
        .and_then(|value| string_field(value, "description"))
        .or_else(|| string_field(&tool.arguments, "description"))
        .unwrap_or_default();
    let html = details
        .and_then(|value| string_field(value, "html"))
        .or_else(|| {
            tool.output_segments.as_ref().map(|segments| {
                segments
                    .iter()
                    .filter(|segment| segment.stream == ToolOutputStream::Stdout)
                    .map(|segment| segment.text.as_str())
                    .collect::<String>()
            })
        })
        .filter(|html| !html.trim().is_empty())
        .or_else(|| {
            let trimmed = tool.output.trim();
            (!trimmed.is_empty()).then_some(trimmed.to_string())
        })
        .or_else(|| extract_widget_html_from_raw_arguments(&tool.raw_arguments))?;
    let content_type = details
        .and_then(|value| string_field(value, "content_type"))
        .unwrap_or_else(|| "text/html".to_string());

    Some(UiWidgetDocument { title, description, html, content_type })
}

pub(crate) fn sync_widget_projection(tool: &mut CurrentToolOutput) {
    tool.widget = derive_widget_document(tool).map(|document| UiWidget {
        instance_id: tool.invocation_id.clone(),
        phase: if tool.completed { UiWidgetPhase::Final } else { UiWidgetPhase::Preview },
        document,
    });
}
