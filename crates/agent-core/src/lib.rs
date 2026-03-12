use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModelDisposition {
    Balanced,
    Precise,
    Fast,
    Creative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub provider: String,
    pub name: String,
    pub disposition: ModelDisposition,
}

impl ModelIdentity {
    pub fn new(
        provider: impl Into<String>,
        name: impl Into<String>,
        disposition: ModelDisposition,
    ) -> Self {
        Self { provider: provider.into(), name: name.into(), disposition }
    }
}

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

    /// Backwards-compatible builder: adds a string parameter to the JSON Schema.
    pub fn with_parameter(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        let description = description.into();
        if let Some(obj) = self.parameters.as_object_mut() {
            if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
                props.insert(
                    name.clone(),
                    serde_json::json!({ "type": "string", "description": description }),
                );
            }
            if required {
                if let Some(req) = obj.get_mut("required").and_then(|v| v.as_array_mut()) {
                    req.push(Value::String(name));
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
}

impl ToolCall {
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            invocation_id: next_tool_invocation_id(),
            tool_name: tool_name.into(),
            arguments: Value::Object(serde_json::Map::new()),
        }
    }

    pub fn with_invocation_id(mut self, invocation_id: impl Into<String>) -> Self {
        self.invocation_id = invocation_id.into();
        self
    }

    /// Backwards-compatible builder: inserts a string key-value into the arguments object.
    pub fn with_argument(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        if let Some(obj) = self.arguments.as_object_mut() {
            obj.insert(name.into(), Value::String(value.into()));
        }
        self
    }

    /// Set the entire arguments value (must be a JSON object).
    pub fn with_arguments_value(mut self, arguments: Value) -> Self {
        self.arguments = arguments;
        self
    }
}

impl PartialEq for ToolCall {
    fn eq(&self, other: &Self) -> bool {
        self.invocation_id == other.invocation_id
            && self.tool_name == other.tool_name
            && self.arguments == other.arguments
    }
}

impl Eq for ToolCall {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub invocation_id: String,
    pub tool_name: String,
    pub content: String,
}

impl ToolResult {
    pub fn from_call(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            invocation_id: call.invocation_id.clone(),
            tool_name: call.tool_name.clone(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompletionSegment {
    Text(String),
    Thinking(String),
    ToolUse(ToolCall),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub segments: Vec<CompletionSegment>,
}

impl Completion {
    pub fn text(content: impl Into<String>) -> Self {
        Self { segments: vec![CompletionSegment::Text(content.into())] }
    }

    pub fn plain_text(&self) -> String {
        self.segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::Text(text) => Some(text.as_str()),
                CompletionSegment::Thinking(_) | CompletionSegment::ToolUse(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn thinking_text(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .segments
            .iter()
            .filter_map(|s| match s {
                CompletionSegment::Thinking(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        if parts.is_empty() { None } else { Some(parts.join("")) }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: ModelIdentity,
    pub instructions: Option<String>,
    pub conversation: Vec<Message>,
    pub available_tools: Vec<ToolDefinition>,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolOutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug)]
pub struct ToolOutputDelta {
    pub stream: ToolOutputStream,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ToolExecutionContext {
    pub run_id: String,
    pub workspace_root: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamEvent {
    ThinkingDelta { text: String },
    TextDelta { text: String },
    ToolOutputDelta {
        invocation_id: String,
        stream: ToolOutputStream,
        text: String,
    },
    Log { text: String },
    Done,
}

pub trait LanguageModel {
    type Error: std::error::Error;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error>;

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        let completion = self.complete(request)?;
        sink(StreamEvent::Done);
        Ok(completion)
    }
}

pub trait ToolExecutor {
    type Error: std::error::Error;

    fn definitions(&self) -> Vec<ToolDefinition>;

    fn call(
        &self,
        call: &ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CoreError {
    message: String,
}

impl CoreError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CoreError {}

fn next_tool_invocation_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间应晚于 UNIX_EPOCH")
        .as_millis();
    format!("tool-call-{now_ms}-{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 工具定义用_json_schema_构建参数() {
        let definition = ToolDefinition::new("search", "搜索代码")
            .with_parameter("query", "要搜索的关键字", true)
            .with_parameter("path", "限定路径", false);

        assert_eq!(definition.parameters["properties"]["query"]["type"], "string");
        assert_eq!(definition.parameters["properties"]["path"]["description"], "限定路径");
        assert_eq!(definition.parameters["required"], serde_json::json!(["query"]));
    }

    #[test]
    fn 完成结果可提取纯文本() {
        let completion = Completion {
            segments: vec![
                CompletionSegment::Text("第一行".into()),
                CompletionSegment::ToolUse(ToolCall::new("search")),
                CompletionSegment::Text("第二行".into()),
            ],
        };

        assert_eq!(completion.plain_text(), "第一行\n第二行");
    }

    #[test]
    fn 工具调用默认生成稳定调用标识() {
        let first = ToolCall::new("search");
        let second = ToolCall::new("search");

        assert_ne!(first.invocation_id, second.invocation_id);
        assert!(first.invocation_id.starts_with("tool-call-"));
    }

    #[test]
    fn 工具调用参数为_json_对象() {
        let call = ToolCall::new("search").with_argument("query", "runtime");

        assert_eq!(call.arguments["query"], "runtime");
    }

    #[test]
    fn 工具结果继承工具调用标识() {
        let call = ToolCall::new("search").with_argument("query", "runtime");
        let result = ToolResult::from_call(&call, "ok");

        assert_eq!(result.invocation_id, call.invocation_id);
        assert_eq!(result.tool_name, "search");
        assert_eq!(result.content, "ok");
    }
}
