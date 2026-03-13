use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CoreError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false,
            }),
        }
    }

    pub fn with_parameter(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        let description = description.into();
        if let Some(obj) = self.parameters.as_object_mut() {
            if let Some(props) = obj.get_mut("properties").and_then(|value| value.as_object_mut()) {
                props.insert(
                    name.clone(),
                    serde_json::json!({ "type": "string", "description": description }),
                );
            }
            if required {
                if let Some(required_fields) =
                    obj.get_mut("required").and_then(|value| value.as_array_mut())
                {
                    required_fields.push(Value::String(name));
                }
            }
        }
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: Value,
    #[serde(default)]
    pub response_id: Option<String>,
}

impl ToolCall {
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            invocation_id: next_tool_invocation_id(),
            tool_name: tool_name.into(),
            arguments: Value::Object(serde_json::Map::new()),
            response_id: None,
        }
    }

    pub fn with_invocation_id(mut self, invocation_id: impl Into<String>) -> Self {
        self.invocation_id = invocation_id.into();
        self
    }

    pub fn with_argument(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        if let Some(obj) = self.arguments.as_object_mut() {
            obj.insert(name.into(), Value::String(value.into()));
        }
        self
    }

    pub fn with_arguments_value(mut self, arguments: Value) -> Self {
        self.arguments = arguments;
        self
    }

    pub fn with_response_id(mut self, response_id: impl Into<String>) -> Self {
        self.response_id = Some(response_id.into());
        self
    }

    pub fn str_arg(&self, name: &str) -> Result<String, CoreError> {
        self.arguments
            .get(name)
            .and_then(|value| value.as_str())
            .map(String::from)
            .ok_or_else(|| CoreError::new(format!("missing required argument: {name}")))
    }

    pub fn opt_str_arg(&self, name: &str) -> Option<String> {
        self.arguments.get(name).and_then(|value| value.as_str()).map(String::from)
    }

    pub fn opt_usize_arg(&self, name: &str) -> Option<usize> {
        self.arguments.get(name).and_then(|value| value.as_u64()).map(|value| value as usize)
    }
}

impl PartialEq for ToolCall {
    fn eq(&self, other: &Self) -> bool {
        self.invocation_id == other.invocation_id
            && self.tool_name == other.tool_name
            && self.arguments == other.arguments
            && self.response_id == other.response_id
    }
}

impl Eq for ToolCall {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub invocation_id: String,
    pub tool_name: String,
    pub content: String,
    #[serde(default)]
    pub response_id: Option<String>,
}

impl ToolResult {
    pub fn from_call(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            invocation_id: call.invocation_id.clone(),
            tool_name: call.tool_name.clone(),
            content: content.into(),
            response_id: call.response_id.clone(),
        }
    }
}

fn next_tool_invocation_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间应晚于 UNIX_EPOCH")
        .as_millis();
    format!("tool-call-{now_ms}-{id}")
}
