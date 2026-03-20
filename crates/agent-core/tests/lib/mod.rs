use std::{
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

use agent_core_macros::ToolArgsSchema;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::*;
use crate::tooling::duration_since_unix_epoch;

#[test]
fn 工具定义用_json_schema_构建参数() {
    let definition = ToolDefinition::new("search", "搜索代码")
        .with_parameter("query", "要搜索的关键字", true)
        .with_parameter("path", "限定路径", false);

    assert_eq!(definition.parameters["properties"]["query"]["type"], "string");
    assert_eq!(definition.parameters["properties"]["path"]["description"], "限定路径");
    assert_eq!(definition.parameters["required"], serde_json::json!(["query"]));
}

#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct SearchArgsSchema {
    #[tool_schema(description = "要搜索的关键字")]
    query: String,
    #[tool_schema(description = "限定返回数量")]
    limit: Option<u32>,
}

#[test]
fn 工具定义可用自研_schema_生成参数() {
    let definition =
        ToolDefinition::new("search", "搜索代码").with_parameters_schema::<SearchArgsSchema>();

    assert!(definition.parameters.get("$schema").is_none());
    assert!(definition.parameters.get("title").is_none());
    assert_eq!(definition.parameters["type"], "object");
    assert_eq!(definition.parameters["properties"]["query"]["type"], "string");
    assert_eq!(definition.parameters["properties"]["query"]["description"], "要搜索的关键字");
    assert_eq!(definition.parameters["properties"]["limit"]["type"], "integer");
    assert!(definition.parameters["properties"]["limit"].get("anyOf").is_none());
    assert_eq!(definition.parameters["required"], serde_json::json!(["query"]));
    assert_eq!(definition.parameters["additionalProperties"], false);
}

#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[serde(deny_unknown_fields)]
#[tool_schema(min_properties = 1)]
struct AliasPatchArgs {
    #[tool_schema(description = "补丁正文")]
    patch: Option<String>,
    #[serde(rename = "patchText")]
    #[tool_schema(description = "补丁正文别名")]
    patch_text: Option<String>,
}

#[test]
fn 自研_schema_可为带别名的可选字段_struct_生成扁平对象参数() {
    let definition =
        ToolDefinition::new("apply_patch", "应用补丁").with_parameters_schema::<AliasPatchArgs>();

    assert_eq!(definition.parameters["type"], "object");
    assert_eq!(definition.parameters["minProperties"], 1);
    assert_eq!(definition.parameters["required"], serde_json::json!([]));
    assert_eq!(definition.parameters["properties"]["patch"]["type"], "string");
    assert_eq!(definition.parameters["properties"]["patchText"]["type"], "string");
    assert_eq!(definition.parameters["properties"]["patchText"]["description"], "补丁正文别名");
}

#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct ExtendedArgsSchema {
    enabled: bool,
    delta: Option<i64>,
    tags: Option<Vec<String>>,
    #[tool_schema(minimum = -5, maximum = 5)]
    balance: i32,
    #[tool_schema(minimum = 1, maximum = 10)]
    attempts: u32,
}

#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct DecoratedArgsSchema {
    #[tool_schema(description = "应用标识", meta(key = "x-label", value = "App ID"))]
    app_id: String,
    #[tool_schema(
        description = "应用密钥",
        meta(key = "x-label", value = "App Secret"),
        meta(key = "x-secret", value = true)
    )]
    app_secret: String,
    #[tool_schema(
        description = "基础地址",
        meta(key = "format", value = "uri"),
        meta(key = "default", value = "https://open.feishu.cn")
    )]
    base_url: Option<String>,
}

#[test]
fn 自研_schema_支持更多高频字段类型与数值约束() {
    let definition =
        ToolDefinition::new("extended", "扩展字段").with_parameters_schema::<ExtendedArgsSchema>();

    assert_eq!(definition.parameters["properties"]["enabled"]["type"], "boolean");
    assert_eq!(definition.parameters["properties"]["delta"]["type"], "integer");
    assert!(definition.parameters["properties"]["delta"].get("minimum").is_none());
    assert_eq!(definition.parameters["properties"]["tags"]["type"], "array");
    assert_eq!(definition.parameters["properties"]["tags"]["items"]["type"], "string");
    assert_eq!(definition.parameters["properties"]["balance"]["minimum"], -5);
    assert_eq!(definition.parameters["properties"]["balance"]["maximum"], 5);
    assert_eq!(definition.parameters["properties"]["attempts"]["minimum"], 1);
    assert_eq!(definition.parameters["properties"]["attempts"]["maximum"], 10);
    assert_eq!(
        definition.parameters["required"],
        serde_json::json!(["enabled", "balance", "attempts"])
    );
}

#[test]
fn 自研_schema_可附加属性级扩展元信息() {
    let definition =
        ToolDefinition::new("channel", "通道配置").with_parameters_schema::<DecoratedArgsSchema>();

    assert_eq!(definition.parameters["properties"]["app_id"]["x-label"], "App ID");
    assert_eq!(definition.parameters["properties"]["app_secret"]["x-label"], "App Secret");
    assert_eq!(definition.parameters["properties"]["app_secret"]["x-secret"], true);
    assert_eq!(definition.parameters["properties"]["base_url"]["format"], "uri");
    assert_eq!(
        definition.parameters["properties"]["base_url"]["default"],
        "https://open.feishu.cn"
    );
}

#[test]
fn 工具定义可直接接收手写参数_schema() {
    let definition =
        ToolDefinition::new("apply_patch", "应用补丁").with_parameters_value(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "patch": { "type": "string" }
            },
            "required": ["patch"],
            "additionalProperties": false
        }));

    assert!(definition.parameters.get("$schema").is_none());
    assert_eq!(definition.parameters["type"], "object");
    assert_eq!(definition.parameters["properties"]["patch"]["type"], "string");
    assert_eq!(definition.parameters["required"], serde_json::json!(["patch"]));
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
fn system_time_before_unix_epoch_falls_back_to_zero_duration() {
    let before_epoch = UNIX_EPOCH - Duration::from_secs(1);

    assert_eq!(duration_since_unix_epoch(before_epoch), Duration::ZERO);
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

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TypedToolArgs {
    name: String,
    limit: Option<usize>,
}

fn parse_args<T: DeserializeOwned>(call: &ToolCall) -> Result<T, CoreError> {
    call.parse_arguments()
}

#[test]
fn parse_arguments_可解析_typed_args() {
    let call =
        ToolCall::new("t").with_arguments_value(serde_json::json!({"name": "hello", "limit": 3}));

    let args: TypedToolArgs = parse_args(&call).unwrap();

    assert_eq!(args, TypedToolArgs { name: "hello".into(), limit: Some(3) });
}

#[test]
fn parse_arguments_类型不匹配时返回错误() {
    let call = ToolCall::new("t").with_arguments_value(serde_json::json!({"name": 3}));

    let error = parse_args::<TypedToolArgs>(&call).unwrap_err();

    assert!(error.to_string().contains("invalid tool arguments"));
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

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new("echo", "回显输入").with_parameter("text", "要回显的文本", true)
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
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

#[tokio::test(flavor = "current_thread")]
async fn 注册表按名称分派工具调用() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool));

    let call = ToolCall::new("echo").with_argument("text", "你好");
    let ctx = ToolExecutionContext {
        run_id: "r1".into(),
        workspace_root: None,
        abort: AbortSignal::new(),
        runtime: None,
    };
    let result = ToolExecutor::call(&registry, &call, &mut |_| {}, &ctx).await.unwrap();
    assert_eq!(result.content, "你好");
}

#[tokio::test(flavor = "current_thread")]
async fn 注册表未知工具返回错误() {
    let registry = ToolRegistry::new();
    let call = ToolCall::new("nonexistent");
    let ctx = ToolExecutionContext {
        run_id: "r1".into(),
        workspace_root: None,
        abort: AbortSignal::new(),
        runtime: None,
    };
    let err = ToolExecutor::call(&registry, &call, &mut |_| {}, &ctx).await.unwrap_err();
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
