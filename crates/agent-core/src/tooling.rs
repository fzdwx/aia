use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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
            if required
                && let Some(required_fields) =
                    obj.get_mut("required").and_then(|value| value.as_array_mut())
            {
                required_fields.push(Value::String(name));
            }
        }
        self
    }

    pub fn with_parameters_schema<T: JsonSchema>(mut self) -> Self {
        self.parameters = normalize_schema_parameters(schemars::schema_for!(T).into());
        self
    }

    pub fn with_parameters_value(mut self, parameters: Value) -> Self {
        self.parameters = normalize_schema_parameters(parameters);
        self
    }
}

fn normalize_schema_parameters(mut schema: Value) -> Value {
    normalize_schema_value(&mut schema);
    schema
}

fn normalize_schema_value(value: &mut Value) {
    match value {
        Value::Object(object) => normalize_schema_object(object),
        Value::Array(items) => {
            for item in items {
                normalize_schema_value(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn normalize_schema_object(object: &mut Map<String, Value>) {
    object.remove("$schema");
    object.remove("title");

    for value in object.values_mut() {
        normalize_schema_value(value);
    }

    normalize_nullable_type(object);
    normalize_nullable_union(object, "anyOf");
    normalize_nullable_union(object, "oneOf");
}

fn normalize_nullable_type(object: &mut Map<String, Value>) {
    let Some(type_value) = object.get_mut("type") else {
        return;
    };
    let Value::Array(type_items) = type_value else {
        return;
    };

    let filtered = type_items
        .iter()
        .filter(|item| !matches!(item, Value::String(name) if name == "null"))
        .cloned()
        .collect::<Vec<_>>();

    if filtered.len() == type_items.len() || filtered.is_empty() {
        return;
    }

    *type_value = if filtered.len() == 1 {
        filtered.into_iter().next().unwrap_or(Value::String("null".into()))
    } else {
        Value::Array(filtered)
    };
}

fn normalize_nullable_union(object: &mut Map<String, Value>, key: &str) {
    let Some(Value::Array(options)) = object.remove(key) else {
        return;
    };

    let mut non_null = Vec::new();
    let mut null_count = 0;
    for option in options {
        if is_null_schema(&option) {
            null_count += 1;
        } else {
            non_null.push(option);
        }
    }

    if null_count == 0 || non_null.is_empty() {
        let mut restored = non_null;
        restored.extend(std::iter::repeat_n(serde_json::json!({ "type": "null" }), null_count));
        object.insert(key.to_string(), Value::Array(restored));
        return;
    }

    if non_null.len() == 1 {
        if let Some(remaining) = non_null.into_iter().next() {
            match remaining {
                Value::Object(branch) => {
                    for (branch_key, branch_value) in branch {
                        object.entry(branch_key).or_insert(branch_value);
                    }
                }
                value => {
                    object.insert(key.to_string(), Value::Array(vec![value]));
                }
            }
        }
        return;
    }

    object.insert(key.to_string(), Value::Array(non_null));
}

fn is_null_schema(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            matches!(object.get("type"), Some(Value::String(name)) if name == "null")
        }
        _ => false,
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

    pub fn parse_arguments<T: DeserializeOwned>(&self) -> Result<T, CoreError> {
        serde_json::from_value(self.arguments.clone())
            .map_err(|error| CoreError::new(format!("invalid tool arguments: {error}")))
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ToolResult {
    pub fn from_call(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            invocation_id: call.invocation_id.clone(),
            tool_name: call.tool_name.clone(),
            content: content.into(),
            response_id: call.response_id.clone(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

pub(crate) fn duration_since_unix_epoch(now: SystemTime) -> Duration {
    now.duration_since(UNIX_EPOCH).unwrap_or_default()
}

fn next_tool_invocation_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = duration_since_unix_epoch(SystemTime::now()).as_millis();
    format!("tool-call-{now_ms}-{id}")
}
