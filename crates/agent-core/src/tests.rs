use std::path::PathBuf;

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
        stop_reason: CompletionStopReason::Stop,
        usage: None,
        response_body: None,
        http_status_code: None,
    };

    assert_eq!(completion.plain_text(), "第一行\n第二行");
}

#[test]
fn 文本完成默认_stop_reason_为_stop() {
    let completion = Completion::text("你好");

    assert_eq!(completion.stop_reason, CompletionStopReason::Stop);
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

#[test]
fn resolve_path_绝对路径直接返回() {
    let ctx = ToolExecutionContext {
        run_id: "r1".into(),
        workspace_root: Some(PathBuf::from("/workspace")),
        abort: AbortSignal::new(),
        runtime: None,
    };
    assert_eq!(ctx.resolve_path("/etc/hosts"), PathBuf::from("/etc/hosts"));
}

#[test]
fn resolve_path_相对路径拼接_workspace_root() {
    let ctx = ToolExecutionContext {
        run_id: "r1".into(),
        workspace_root: Some(PathBuf::from("/workspace")),
        abort: AbortSignal::new(),
        runtime: None,
    };
    assert_eq!(ctx.resolve_path("src/main.rs"), PathBuf::from("/workspace/src/main.rs"));
}

#[test]
fn resolve_path_无_workspace_root_时返回原样() {
    let ctx = ToolExecutionContext {
        run_id: "r1".into(),
        workspace_root: None,
        abort: AbortSignal::new(),
        runtime: None,
    };
    assert_eq!(ctx.resolve_path("src/main.rs"), PathBuf::from("src/main.rs"));
}

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
        runtime: None,
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
        runtime: None,
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
