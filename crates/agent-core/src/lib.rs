use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
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
pub enum ConversationItem {
    Message(Message),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

impl ConversationItem {
    pub fn message(role: Role, content: impl Into<String>) -> Self {
        Self::Message(Message::new(role, content))
    }

    pub fn as_message(&self) -> Option<&Message> {
        match self {
            Self::Message(message) => Some(message),
            Self::ToolCall(_) | Self::ToolResult(_) => None,
        }
    }

    pub fn as_tool_call(&self) -> Option<&ToolCall> {
        match self {
            Self::ToolCall(call) => Some(call),
            Self::Message(_) | Self::ToolResult(_) => None,
        }
    }

    pub fn as_tool_result(&self) -> Option<&ToolResult> {
        match self {
            Self::ToolResult(result) => Some(result),
            Self::Message(_) | Self::ToolCall(_) => None,
        }
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

    pub fn with_response_id(mut self, response_id: impl Into<String>) -> Self {
        self.response_id = Some(response_id.into());
        self
    }
}

impl ToolCall {
    /// Extract a required string argument, returning `CoreError` if missing.
    pub fn str_arg(&self, name: &str) -> Result<String, CoreError> {
        self.arguments
            .get(name)
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| CoreError::new(format!("missing required argument: {name}")))
    }

    /// Extract an optional string argument.
    pub fn opt_str_arg(&self, name: &str) -> Option<String> {
        self.arguments.get(name).and_then(|v| v.as_str()).map(String::from)
    }

    /// Extract an optional usize argument (from a JSON integer).
    pub fn opt_usize_arg(&self, name: &str) -> Option<usize> {
        self.arguments.get(name).and_then(|v| v.as_u64()).map(|n| n as usize)
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelCheckpoint {
    pub protocol: String,
    pub token: String,
}

impl ModelCheckpoint {
    pub fn new(protocol: impl Into<String>, token: impl Into<String>) -> Self {
        Self { protocol: protocol.into(), token: token.into() }
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
    pub checkpoint: Option<ModelCheckpoint>,
}

impl Completion {
    pub fn text(content: impl Into<String>) -> Self {
        Self { segments: vec![CompletionSegment::Text(content.into())], checkpoint: None }
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
    pub conversation: Vec<ConversationItem>,
    pub resume_checkpoint: Option<ModelCheckpoint>,
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
pub struct AbortSignal(Arc<AtomicBool>);

impl AbortSignal {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn abort(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_aborted(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct ToolExecutionContext {
    pub run_id: String,
    pub workspace_root: Option<std::path::PathBuf>,
    pub abort: AbortSignal,
}

impl ToolExecutionContext {
    /// Resolve a raw path: absolute paths pass through; relative paths are
    /// joined to `workspace_root` (if set), otherwise returned as-is.
    pub fn resolve_path(&self, raw: &str) -> PathBuf {
        let path = Path::new(raw);
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(root) = &self.workspace_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamEvent {
    ThinkingDelta { text: String },
    TextDelta { text: String },
    ToolOutputDelta { invocation_id: String, stream: ToolOutputStream, text: String },
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

// ---------------------------------------------------------------------------
// Tool trait + ToolRegistry
// ---------------------------------------------------------------------------

/// A single self-contained tool: provides its own definition and execution logic.
pub trait Tool: Send {
    fn name(&self) -> &str;

    fn definition(&self) -> ToolDefinition;

    fn call(
        &self,
        tool_call: &ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError>;
}

/// A collection of `Tool` objects, addressable by name. Implements `ToolExecutor`.
pub struct ToolRegistry {
    tools: BTreeMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: BTreeMap::new() }
    }

    /// Register a tool. If a tool with the same name already exists, the old
    /// one is returned.
    pub fn register(&mut self, tool: Box<dyn Tool>) -> Option<Box<dyn Tool>> {
        let name = tool.name().to_owned();
        self.tools.insert(name, tool)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExecutor for ToolRegistry {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    fn call(
        &self,
        call: &ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        match self.tools.get(&call.tool_name) {
            Some(tool) => tool.call(call, output, context),
            None => Err(CoreError::new(format!("unknown tool: {}", call.tool_name))),
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
            checkpoint: None,
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

    // --- ToolCall helper methods ---

    #[test]
    fn str_arg_返回必填字符串参数() {
        let call = ToolCall::new("t").with_argument("name", "hello");
        assert_eq!(call.str_arg("name").unwrap(), "hello");
    }

    #[test]
    fn str_arg_缺失时返回错误() {
        let call = ToolCall::new("t");
        assert!(call.str_arg("missing").is_err());
    }

    #[test]
    fn opt_str_arg_返回存在的参数() {
        let call = ToolCall::new("t").with_argument("path", "/tmp");
        assert_eq!(call.opt_str_arg("path"), Some("/tmp".to_string()));
        assert_eq!(call.opt_str_arg("missing"), None);
    }

    #[test]
    fn opt_usize_arg_解析整数参数() {
        let call = ToolCall::new("t").with_arguments_value(serde_json::json!({"limit": 100}));
        assert_eq!(call.opt_usize_arg("limit"), Some(100));
        assert_eq!(call.opt_usize_arg("missing"), None);
    }

    // --- ToolExecutionContext::resolve_path ---

    #[test]
    fn resolve_path_绝对路径直接返回() {
        let ctx = ToolExecutionContext {
            run_id: "r1".into(),
            workspace_root: Some(PathBuf::from("/workspace")),
            abort: AbortSignal::new(),
        };
        assert_eq!(ctx.resolve_path("/etc/hosts"), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn resolve_path_相对路径拼接_workspace_root() {
        let ctx = ToolExecutionContext {
            run_id: "r1".into(),
            workspace_root: Some(PathBuf::from("/workspace")),
            abort: AbortSignal::new(),
        };
        assert_eq!(ctx.resolve_path("src/main.rs"), PathBuf::from("/workspace/src/main.rs"));
    }

    #[test]
    fn resolve_path_无_workspace_root_时返回原样() {
        let ctx = ToolExecutionContext {
            run_id: "r1".into(),
            workspace_root: None,
            abort: AbortSignal::new(),
        };
        assert_eq!(ctx.resolve_path("src/main.rs"), PathBuf::from("src/main.rs"));
    }

    // --- ToolRegistry ---

    struct EchoTool;

    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn definition(&self) -> ToolDefinition {
            ToolDefinition::new("echo", "回显输入").with_parameter("text", "要回显的文本", true)
        }

        fn call(
            &self,
            tool_call: &ToolCall,
            _output: &mut dyn FnMut(ToolOutputDelta),
            _context: &ToolExecutionContext,
        ) -> Result<ToolResult, CoreError> {
            let text = tool_call.str_arg("text")?;
            Ok(ToolResult::from_call(tool_call, text))
        }
    }

    #[test]
    fn 注册表收集工具定义() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let defs = registry.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
    }

    #[test]
    fn 注册表按名称分派工具调用() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));

        let call = ToolCall::new("echo").with_argument("text", "你好");
        let ctx = ToolExecutionContext {
            run_id: "r1".into(),
            workspace_root: None,
            abort: AbortSignal::new(),
        };
        let result = ToolExecutor::call(&registry, &call, &mut |_| {}, &ctx).unwrap();
        assert_eq!(result.content, "你好");
    }

    #[test]
    fn 注册表未知工具返回错误() {
        let registry = ToolRegistry::new();
        let call = ToolCall::new("nonexistent");
        let ctx = ToolExecutionContext {
            run_id: "r1".into(),
            workspace_root: None,
            abort: AbortSignal::new(),
        };
        let err = ToolExecutor::call(&registry, &call, &mut |_| {}, &ctx).unwrap_err();
        assert!(err.to_string().contains("unknown tool"));
    }

    #[test]
    fn 注册表同名覆盖返回旧工具() {
        let mut registry = ToolRegistry::new();
        assert!(registry.register(Box::new(EchoTool)).is_none());
        assert!(registry.register(Box::new(EchoTool)).is_some());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn 空注册表返回空定义列表() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.definitions().is_empty());
    }

    // --- AbortSignal ---

    #[test]
    fn abort_signal_初始未中止() {
        let signal = AbortSignal::new();
        assert!(!signal.is_aborted());
    }

    #[test]
    fn abort_signal_触发后可检测() {
        let signal = AbortSignal::new();
        let cloned = signal.clone();
        signal.abort();
        assert!(cloned.is_aborted());
    }
}
