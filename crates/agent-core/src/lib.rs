use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

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
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
}

impl ToolDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self { name: name.into(), description: description.into(), parameters: Vec::new() }
    }

    pub fn with_parameter(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        self.parameters.push(ToolParameter {
            name: name.into(),
            description: description.into(),
            required,
        });
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: Vec<(String, String)>,
}

impl ToolCall {
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            invocation_id: next_tool_invocation_id(),
            tool_name: tool_name.into(),
            arguments: Vec::new(),
        }
    }

    pub fn with_invocation_id(mut self, invocation_id: impl Into<String>) -> Self {
        self.invocation_id = invocation_id.into();
        self
    }

    pub fn with_argument(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.arguments.push((name.into(), value.into()));
        self
    }

    pub fn with_arguments(mut self, arguments: Vec<(String, String)>) -> Self {
        self.arguments = arguments;
        self
    }
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamEvent {
    ThinkingDelta { text: String },
    TextDelta { text: String },
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
    fn call(&self, call: &ToolCall) -> Result<ToolResult, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ToolSpecTarget {
    Internal,
    Claude,
    Codex,
    Mcp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortableToolSpec {
    pub target: ToolSpecTarget,
    pub name: String,
    pub description: String,
    pub parameter_names: Vec<String>,
}

impl PortableToolSpec {
    pub fn from_definition(definition: &ToolDefinition, target: ToolSpecTarget) -> Self {
        Self {
            target,
            name: definition.name.clone(),
            description: definition.description.clone(),
            parameter_names: definition
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
        }
    }
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
    fn 便携工具规范保留参数名() {
        let definition = ToolDefinition::new("search", "搜索代码")
            .with_parameter("query", "要搜索的关键字", true)
            .with_parameter("path", "限定路径", false);

        let spec = PortableToolSpec::from_definition(&definition, ToolSpecTarget::Mcp);

        assert_eq!(spec.name, "search");
        assert_eq!(spec.parameter_names, vec!["query", "path"]);
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
    fn 工具结果继承工具调用标识() {
        let call = ToolCall::new("search").with_argument("query", "runtime");
        let result = ToolResult::from_call(&call, "ok");

        assert_eq!(result.invocation_id, call.invocation_id);
        assert_eq!(result.tool_name, "search");
        assert_eq!(result.content, "ok");
    }
}
