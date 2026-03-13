use std::cell::RefCell;

use agent_core::{
    Completion, CompletionRequest, CompletionSegment, ConversationItem, CoreError, LanguageModel,
    Message, ModelCheckpoint, ModelDisposition, ModelIdentity, Role, ToolCall, ToolDefinition,
    ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolResult,
};
use serde_json::json;
use session_tape::SessionTape;

use super::{AgentRuntime, RuntimeEvent};
use crate::{ToolInvocationLifecycle, ToolInvocationOutcome, TurnLifecycle};

struct StubModel;

impl LanguageModel for StubModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let last_user_index = request
            .conversation
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| item.as_message().is_some_and(|message| message.role == Role::User))
            .map(|(index, _)| index);
        let latest_user = request
            .conversation
            .iter()
            .rev()
            .find_map(|item| {
                item.as_message()
                    .filter(|message| message.role == Role::User)
                    .map(|message| message.content.clone())
            })
            .unwrap_or_else(|| "空输入".into());
        let saw_tool_result = last_user_index
            .map(|index| {
                request
                    .conversation
                    .iter()
                    .skip(index + 1)
                    .any(|item| item.as_tool_result().is_some())
            })
            .unwrap_or(false);
        if saw_tool_result {
            return Ok(Completion::text(format!("已收到：{latest_user}")));
        }

        let latest = request
            .conversation
            .last()
            .map(|item| match item {
                ConversationItem::Message(message) => message.content.clone(),
                ConversationItem::ToolCall(call) => format!("工具调用 {}", call.tool_name),
                ConversationItem::ToolResult(result) => result.content.clone(),
            })
            .unwrap_or_else(|| "空输入".into());
        Ok(Completion {
            segments: vec![
                CompletionSegment::Text(format!("准备处理：{latest}")),
                CompletionSegment::ToolUse(ToolCall::new("search")),
            ],
            checkpoint: None,
        })
    }
}

struct FailingModel;

impl LanguageModel for FailingModel {
    type Error = CoreError;

    fn complete(&self, _request: CompletionRequest) -> Result<Completion, Self::Error> {
        Err(CoreError::new("模拟失败"))
    }
}

struct RecordingModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl RecordingModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

struct ContinueAfterToolModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl ContinueAfterToolModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for ContinueAfterToolModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let step = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request.clone());
        if step == 0 {
            Ok(Completion {
                segments: vec![
                    CompletionSegment::Thinking("先查一下".into()),
                    CompletionSegment::ToolUse(ToolCall::new("search")),
                ],
                checkpoint: None,
            })
        } else {
            let saw_tool = request.conversation.iter().any(|item| {
                item.as_tool_result().is_some_and(|result| result.content.contains("未实现"))
            });
            if saw_tool {
                Ok(Completion::text("已根据工具结果继续回答"))
            } else {
                Err(CoreError::new("未看到工具结果"))
            }
        }
    }
}

struct DuplicateToolLoopModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl DuplicateToolLoopModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for DuplicateToolLoopModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        self.seen_requests.borrow_mut().push(request.clone());
        let saw_duplicate_skip = request.conversation.iter().any(|item| {
            item.as_tool_result()
                .is_some_and(|result| result.content.contains("重复工具调用已跳过"))
        });
        if saw_duplicate_skip {
            return Ok(Completion::text("已停止重复调用并给出最终回答"));
        }

        let saw_initial_tool_result = request.conversation.iter().any(|item| {
            item.as_tool_result().is_some_and(|result| result.content.contains("未实现"))
        });

        if saw_initial_tool_result {
            return Ok(Completion {
                segments: vec![CompletionSegment::ToolUse(
                    ToolCall::new("search").with_argument("query", "date"),
                )],
                checkpoint: None,
            });
        }

        Ok(Completion {
            segments: vec![CompletionSegment::ToolUse(
                ToolCall::new("search").with_argument("query", "date"),
            )],
            checkpoint: None,
        })
    }
}

struct FailingTools;

impl ToolExecutor for FailingTools {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new("search", "搜索代码")]
    }

    fn call(
        &self,
        _call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        _context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error> {
        Err(CoreError::new("工具炸了"))
    }
}

impl LanguageModel for RecordingModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        self.seen_requests.borrow_mut().push(request);
        Ok(Completion::text("记录完成"))
    }
}

struct CheckpointRecordingModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl CheckpointRecordingModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for CheckpointRecordingModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let index = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request);
        Ok(Completion {
            segments: vec![CompletionSegment::Text(format!("第{}轮完成", index + 1))],
            checkpoint: Some(ModelCheckpoint::new(
                "openai-responses",
                format!("resp_{}", index + 1),
            )),
        })
    }
}

struct StubTools;

impl ToolExecutor for StubTools {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new("search", "搜索代码")]
    }

    fn call(
        &self,
        call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        _context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error> {
        Ok(ToolResult::from_call(call, "未实现"))
    }
}

struct MismatchedTools;

impl ToolExecutor for MismatchedTools {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new("search", "搜索代码")]
    }

    fn call(
        &self,
        _call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        _context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error> {
        Ok(ToolResult {
            invocation_id: "wrong-id".into(),
            tool_name: "search".into(),
            content: "未实现".into(),
            response_id: None,
            details: None,
        })
    }
}

#[test]
fn 运行时会记录用户与助手消息() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime =
        AgentRuntime::new(StubModel, StubTools, identity).with_instructions("保持简洁");

    let output = runtime.handle_turn("你好").expect("运行成功");

    assert_eq!(output.assistant_text, "已收到：你好");
    assert_eq!(runtime.tape().entries().len(), 6);
    assert_eq!(output.visible_tools.len(), 1);
}

#[test]
fn 运行时可生成交接() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
    runtime.handle_turn("开始").expect("运行成功");

    let handoff =
        runtime.handoff("handoff", json!({"summary": "发现阶段结束", "next_steps": ["进入实现"]}));

    assert_eq!(handoff.anchor.state.get("summary").and_then(|v| v.as_str()), Some("发现阶段结束"),);
    assert_eq!(
        handoff.anchor.state.get("next_steps").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(1),
    );
    assert!(handoff.event_id > handoff.anchor.entry_id);
}

#[test]
fn 已禁用工具会作为失败结果写回上下文() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);

    runtime.disable_tool("search");

    let output = runtime.handle_turn("你好").expect("应写回失败结果并继续完成");

    assert_eq!(output.assistant_text, "已收到：你好");
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry.as_tool_result().map(|result| result.content.contains("工具不可用")).unwrap_or(false)
    }));
}

#[test]
fn 多轮调用会保留上下文() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);

    runtime.handle_turn("第一轮").expect("第一轮成功");
    let output = runtime.handle_turn("第二轮").expect("第二轮成功");

    assert_eq!(output.assistant_text, "已收到：第二轮");
    assert_eq!(runtime.tape().entries().len(), 12);
    assert_eq!(
        runtime.tape().entries()[0].as_message().map(|value| value.content.clone()),
        Some("第一轮".into())
    );
    assert_eq!(
        runtime.tape().entries()[6].as_message().map(|value| value.content.clone()),
        Some("第二轮".into())
    );
}

#[test]
fn 同一轮内工具完成后会继续再次调用模型() {
    let identity = ModelIdentity::new("local", "continue", ModelDisposition::Balanced);
    let model = ContinueAfterToolModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let output = runtime.handle_turn("开始").expect("应继续完成");

    assert_eq!(output.assistant_text, "已根据工具结果继续回答");
    assert_eq!(runtime.model.seen_requests.borrow().len(), 2);
    assert!(runtime.tape().entries().iter().any(|entry| entry.as_tool_result().is_some()));
    let second_request = &runtime.model.seen_requests.borrow()[1];
    assert!(second_request.conversation.iter().any(
        |item| matches!(item, ConversationItem::ToolCall(call) if call.tool_name == "search")
    ));
    assert!(second_request.conversation.iter().any(
        |item| matches!(item, ConversationItem::ToolResult(result) if result.tool_name == "search")
    ));
}

#[test]
fn 工具失败不会直接让整轮报错() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, FailingTools, identity);

    let output = runtime.handle_turn("你好").expect("工具失败应写入轮次而不是直接报错");

    assert_eq!(output.assistant_text, "已收到：你好");
    assert!(runtime.tape().entries().iter().any(|entry| entry.as_tool_result().is_some()));
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry
            .as_tool_result()
            .map(|result| result.content.contains("工具执行失败"))
            .unwrap_or(false)
    }));
}

#[test]
fn 同一轮内相同工具与参数的重复调用会被跳过() {
    let identity = ModelIdentity::new("local", "duplicate", ModelDisposition::Balanced);
    let model = DuplicateToolLoopModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let output = runtime.handle_turn("今天星期几").expect("应在跳过重复调用后完成");

    assert_eq!(output.assistant_text, "已停止重复调用并给出最终回答");
    let duplicate_results = runtime
        .tape()
        .entries()
        .iter()
        .filter_map(|entry| entry.as_tool_result())
        .filter(|result| result.content.contains("重复工具调用已跳过"))
        .count();
    assert_eq!(duplicate_results, 1);
}

#[test]
fn 可通过_builder_覆盖默认步数上限() {
    let identity = ModelIdentity::new("local", "duplicate", ModelDisposition::Balanced);
    let model = DuplicateToolLoopModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity).with_max_turn_steps(2);

    let error = runtime.handle_turn("今天星期几").expect_err("两步上限应触发失败");

    assert!(error.to_string().contains("轮次超过最大内部步骤数：2"));
}

#[test]
fn 最后一步会切换为文本收尾模式() {
    let identity = ModelIdentity::new("local", "continue", ModelDisposition::Balanced);
    let model = ContinueAfterToolModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity).with_max_turn_steps(2);

    let output = runtime.handle_turn("开始").expect("最后一步应收尾成功");

    assert_eq!(output.assistant_text, "已根据工具结果继续回答");
    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].available_tools.is_empty());
    assert!(
        requests[1].instructions.as_deref().is_some_and(|text| text.contains("不要再调用任何工具"))
    );
}

#[test]
fn 非最后一步会向模型注入剩余预算提示() {
    let identity = ModelIdentity::new("local", "continue", ModelDisposition::Balanced);
    let model = ContinueAfterToolModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity).with_max_turn_steps(3);

    let _ = runtime.handle_turn("开始").expect("应成功完成");

    let requests = runtime.model.seen_requests.borrow();
    assert!(
        requests[0].instructions.as_deref().is_some_and(|text| text.contains("当前为第 1/3 步")
            && text.contains("剩余可继续调用工具的步数为 2"))
    );
}

#[test]
fn 可通过_builder_限制单轮最大工具调用次数() {
    let identity = ModelIdentity::new("local", "duplicate", ModelDisposition::Balanced);
    let model = DuplicateToolLoopModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity)
        .with_max_turn_steps(10)
        .with_max_tool_calls_per_turn(1);

    let error = runtime.handle_turn("今天星期几").expect_err("超过工具调用上限应失败");

    assert!(error.to_string().contains("轮次超过最大工具调用次数：1"));
}

#[test]
fn 模型失败时当前轮只保留用户消息() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(FailingModel, StubTools, identity);

    let error = runtime.handle_turn("会失败").expect_err("应当失败");

    assert!(error.to_string().contains("模型执行失败"));
    assert_eq!(runtime.tape().entries().len(), 2);
    assert_eq!(
        runtime.tape().entries()[0].as_message().map(|value| value.content.clone()),
        Some("会失败".into())
    );
    assert!(runtime.tape().entries()[1].event_name().is_some());
}

#[test]
fn 默认上下文会从最新锚点之后重建() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);

    runtime.handle_turn("第一轮").expect("第一轮成功");
    let _ = runtime.handoff(
        "handoff",
        json!({"phase": "handoff", "summary": "切到实现阶段", "next_steps": ["继续执行"], "source_entry_ids": [], "owner": "agent"}),
    );
    runtime.handle_turn("第二轮").expect("第二轮成功");

    let default_messages = runtime.tape().default_messages();

    assert_eq!(default_messages.len(), 4);
    assert_eq!(default_messages[0].content, "第二轮");
    assert_eq!(default_messages[1].content, "准备处理：第二轮");
    assert!(default_messages[2].content.starts_with("工具 search #tool-call-"));
    assert!(default_messages[2].content.ends_with("输出: 未实现"));
    assert_eq!(default_messages[3].content, "已收到：第二轮");
}

#[test]
fn 锚点状态会注入后续请求上下文() {
    let identity = ModelIdentity::new("local", "recording", ModelDisposition::Balanced);
    let model = RecordingModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    runtime.handle_turn("第一轮").expect("第一轮成功");
    let _ = runtime.handoff(
        "handoff",
        json!({"phase": "handoff", "summary": "切到实现阶段", "next_steps": ["继续执行"], "source_entry_ids": [], "owner": "agent"}),
    );
    runtime.handle_turn("第二轮").expect("第二轮成功");

    let requests = runtime.model.seen_requests.borrow();
    let last_request = requests.last().expect("应记录第二轮请求");

    assert!(matches!(
        &last_request.conversation[0],
        ConversationItem::Message(message)
            if message.role == Role::System
                && message.content.contains("当前阶段: handoff")
                && message.content.contains("锚点摘要: 切到实现阶段")
    ));
    assert!(matches!(
        &last_request.conversation[1],
        ConversationItem::Message(message) if message.content == "第二轮"
    ));
}

#[test]
fn 载入现有磁带后会继续沿用已保存上下文() {
    let identity = ModelIdentity::new("local", "recording", ModelDisposition::Balanced);
    let model = RecordingModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "历史用户消息"));
    tape.append(Message::new(Role::Assistant, "历史助手消息"));

    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);
    runtime.handle_turn("新的输入").expect("运行成功");

    let requests = runtime.model.seen_requests.borrow();
    let last_request = requests.last().expect("应记录新请求");

    assert!(matches!(
        &last_request.conversation[0],
        ConversationItem::Message(message) if message.content == "历史用户消息"
    ));
    assert!(matches!(
        &last_request.conversation[1],
        ConversationItem::Message(message) if message.content == "历史助手消息"
    ));
    assert!(matches!(
        &last_request.conversation[2],
        ConversationItem::Message(message) if message.content == "新的输入"
    ));
}

#[test]
fn responses_检查点存在时下一轮只发送增量上下文() {
    let identity = ModelIdentity::new("openai", "responses", ModelDisposition::Balanced);
    let model = CheckpointRecordingModel::new();
    let mut tape = SessionTape::new();
    tape.bind_provider(session_tape::SessionProviderBinding::Provider {
        name: "resp".into(),
        model: "gpt-4.1-mini".into(),
        base_url: "https://api.openai.com/v1".into(),
        protocol: "openai-responses".into(),
    });
    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    runtime.handle_turn("第一轮").expect("第一轮成功");
    runtime.handle_turn("第二轮").expect("第二轮成功");

    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].resume_checkpoint.as_ref().is_some_and(|checkpoint| checkpoint.protocol
        == "openai-responses"
        && checkpoint.token == "resp_1"));
    assert_eq!(requests[1].conversation.len(), 1);
    assert!(matches!(
        &requests[1].conversation[0],
        ConversationItem::Message(message) if message.role == Role::User && message.content == "第二轮"
    ));
}

#[test]
fn 多个订阅者可各自拿到同一批事件() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
    let first = runtime.subscribe();
    let second = runtime.subscribe();

    runtime.handle_turn("你好").expect("运行成功");

    let first_events = runtime.collect_events(first).expect("读取事件成功");
    let second_events = runtime.collect_events(second).expect("读取事件成功");

    assert_eq!(first_events, second_events);
    assert!(first_events.contains(&RuntimeEvent::UserMessage { content: "你好".into() }));
    assert!(
        first_events
            .contains(&RuntimeEvent::AssistantMessage { content: "已收到：你好".into() })
    );
    assert!(first_events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ToolInvocation { call, outcome: ToolInvocationOutcome::Succeeded { result } }
            if call == &runtime.tape().entries()[2].as_tool_call().expect("应有工具调用")
            && result == &runtime.tape().entries()[3].as_tool_result().expect("应有工具结果")
    )));
}

#[test]
fn 统一方法读取事件后同一订阅者不会重复拿到旧事件() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
    let subscriber = runtime.subscribe();

    runtime.handle_turn("你好").expect("运行成功");
    let first = runtime.collect_events(subscriber).expect("第一次读取成功");
    let second = runtime.collect_events(subscriber).expect("第二次读取成功");

    assert!(!first.is_empty());
    assert!(second.is_empty());
}

#[test]
fn 失败轮也会发出失败事件() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(FailingModel, StubTools, identity);
    let subscriber = runtime.subscribe();

    let _ = runtime.handle_turn("会失败");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert!(events.contains(&RuntimeEvent::UserMessage { content: "会失败".into() }));
    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::TurnFailed { message } if message.contains("模型执行失败")
    )));
    assert!(runtime.tape().entries()[1].event_name().is_some());
}

#[test]
fn 工具调用与结果会写入磁带并发出事件() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
    let subscriber = runtime.subscribe();

    let _ = runtime.handle_turn("你好").expect("运行成功");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert!(runtime.tape().entries().iter().any(|entry| entry.as_tool_call().is_some()));
    assert!(runtime.tape().entries().iter().any(|entry| entry.as_tool_result().is_some()));
    assert_eq!(
        events.iter().filter(|event| matches!(event, RuntimeEvent::ToolInvocation { .. })).count(),
        1
    );
    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ToolInvocation { call, outcome: ToolInvocationOutcome::Succeeded { result } }
            if call == &runtime.tape().entries()[2].as_tool_call().expect("应有工具调用")
            && result == &runtime.tape().entries()[3].as_tool_result().expect("应有工具结果")
    )));
    assert_eq!(
        runtime.tape().entries()[2].as_tool_call().expect("应有工具调用").invocation_id,
        runtime.tape().entries()[3].as_tool_result().expect("应有工具结果").invocation_id,
    );
}

#[test]
fn 禁用工具后即使模型建议也不会执行() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
    let subscriber = runtime.subscribe();
    runtime.disable_tool("search");

    let output = runtime.handle_turn("你好").expect("应当继续完成");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert_eq!(output.assistant_text, "已收到：你好");
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry.as_tool_result().map(|result| result.content.contains("工具不可用")).unwrap_or(false)
    }));
    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ToolInvocation { call, outcome: ToolInvocationOutcome::Failed { message } }
            if call.tool_name == "search" && message.contains("工具不可用")
    )));
}

#[test]
fn 工具结果调用标识错配时会作为失败结果保留() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, MismatchedTools, identity);
    let subscriber = runtime.subscribe();

    let output = runtime.handle_turn("你好").expect("应当继续完成");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert_eq!(output.assistant_text, "已收到：你好");
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry
            .as_tool_result()
            .map(|result| result.content.contains("工具结果不匹配"))
            .unwrap_or(false)
    }));
    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ToolInvocation { call, outcome: ToolInvocationOutcome::Failed { message } }
            if call.tool_name == "search" && message.contains("工具结果不匹配")
    )));
}

#[test]
fn 成功轮会聚合成完整轮次块事件() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
    let subscriber = runtime.subscribe();

    let _ = runtime.handle_turn("你好").expect("运行成功");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::TurnLifecycle {
            turn: TurnLifecycle {
                turn_id,
                started_at_ms,
                finished_at_ms,
                source_entry_ids,
                user_message,
                blocks: _,
                assistant_message: Some(assistant_message),
                thinking: None,
                tool_invocations,
                failure_message: None,
            }
        }
            if turn_id.starts_with("turn-")
            && started_at_ms <= finished_at_ms
            && !source_entry_ids.is_empty()
            && user_message == "你好"
            && assistant_message == "已收到：你好"
            && tool_invocations.len() == 1
            && matches!(
                &tool_invocations[0],
                ToolInvocationLifecycle {
                    call,
                    outcome: ToolInvocationOutcome::Succeeded { result },
                } if result.invocation_id == call.invocation_id
            )
    )));
}

#[test]
fn 工具失败后成功收尾的轮次也会聚合完整块事件() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, MismatchedTools, identity);
    let subscriber = runtime.subscribe();

    let _ = runtime.handle_turn("你好").expect("应继续完成");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::TurnLifecycle {
            turn: TurnLifecycle {
                turn_id,
                started_at_ms,
                finished_at_ms,
                source_entry_ids,
                user_message,
                blocks: _,
                assistant_message: Some(assistant_message),
                thinking: _,
                tool_invocations,
                failure_message: None,
            }
        }
            if turn_id.starts_with("turn-")
            && started_at_ms <= finished_at_ms
            && !source_entry_ids.is_empty()
            && user_message == "你好"
            && assistant_message == "已收到：你好"
            && tool_invocations.len() == 1
            && matches!(
                &tool_invocations[0],
                ToolInvocationLifecycle {
                    call,
                    outcome: ToolInvocationOutcome::Failed { message },
                } if call.tool_name == "search" && message.contains("工具结果不匹配")
            )
    )));
}
