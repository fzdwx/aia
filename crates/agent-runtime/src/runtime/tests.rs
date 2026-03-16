use std::cell::RefCell;

use agent_core::{
    AbortSignal, Completion, CompletionRequest, CompletionSegment, CompletionStopReason,
    CompletionUsage, ConversationItem, CoreError, LanguageModel, Message, ModelDisposition,
    ModelIdentity, Role, ToolCall, ToolDefinition, ToolExecutionContext, ToolExecutor,
    ToolOutputDelta, ToolResult,
};
use serde_json::json;
use session_tape::SessionTape;

use super::{AgentRuntime, RuntimeEvent};
use crate::{
    ToolInvocationLifecycle, ToolInvocationOutcome, TurnControl, TurnLifecycle, TurnOutcome,
};

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
            stop_reason: CompletionStopReason::ToolUse,
            usage: None,
            response_body: None,
            http_status_code: None,
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

struct UsageModel;

impl LanguageModel for UsageModel {
    type Error = CoreError;

    fn complete(&self, _request: CompletionRequest) -> Result<Completion, Self::Error> {
        Ok(Completion {
            segments: vec![CompletionSegment::Text("带 usage 的回答".into())],
            stop_reason: CompletionStopReason::Stop,
            usage: Some(CompletionUsage {
                input_tokens: 21,
                output_tokens: 9,
                total_tokens: 30,
                cached_tokens: 0,
            }),
            response_body: None,
            http_status_code: None,
        })
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
                stop_reason: CompletionStopReason::ToolUse,
                usage: None,
                response_body: None,
                http_status_code: None,
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

struct ManyToolRoundsModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl ManyToolRoundsModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for ManyToolRoundsModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let step = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request.clone());

        let saw_latest_tool_result = request
            .conversation
            .iter()
            .rev()
            .find_map(|item| item.as_tool_result().map(|result| result.content.clone()));

        if step >= 9 && saw_latest_tool_result.is_some() {
            return Ok(Completion::text("超过旧默认步数后仍成功收尾"));
        }

        Ok(Completion {
            segments: vec![CompletionSegment::ToolUse(
                ToolCall::new("search").with_argument("query", format!("step-{step}")),
            )],
            stop_reason: CompletionStopReason::ToolUse,
            usage: None,
            response_body: None,
            http_status_code: None,
        })
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
                stop_reason: CompletionStopReason::ToolUse,
                usage: None,
                response_body: None,
                http_status_code: None,
            });
        }

        Ok(Completion {
            segments: vec![CompletionSegment::ToolUse(
                ToolCall::new("search").with_argument("query", "date"),
            )],
            stop_reason: CompletionStopReason::ToolUse,
            usage: None,
            response_body: None,
            http_status_code: None,
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

struct BudgetRecordingModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl BudgetRecordingModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for BudgetRecordingModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        self.seen_requests.borrow_mut().push(request);
        Ok(Completion::text("预算检查完成"))
    }
}

struct RequestRecordingModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl RequestRecordingModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for RequestRecordingModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let index = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request);
        Ok(Completion {
            segments: vec![CompletionSegment::Text(format!("第{}轮完成", index + 1))],
            stop_reason: CompletionStopReason::Stop,
            usage: None,
            response_body: None,
            http_status_code: None,
        })
    }
}

struct StopReasonDrivenModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl StopReasonDrivenModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for StopReasonDrivenModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let index = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request);

        if index == 0 {
            Ok(Completion {
                segments: vec![CompletionSegment::ToolUse(ToolCall::new("search"))],
                stop_reason: CompletionStopReason::ToolUse,
                usage: None,
                response_body: None,
                http_status_code: None,
            })
        } else {
            Ok(Completion {
                segments: vec![CompletionSegment::Text("按 stop reason 收尾".into())],
                stop_reason: CompletionStopReason::Stop,
                usage: None,
                response_body: None,
                http_status_code: None,
            })
        }
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

struct BlockingCancelAwareTools;

impl ToolExecutor for BlockingCancelAwareTools {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new("search", "搜索代码")]
    }

    fn call(
        &self,
        call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error> {
        for _ in 0..200 {
            if context.abort.is_aborted() {
                return Ok(ToolResult::from_call(call, "[aborted]"));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(ToolResult::from_call(call, "finished without cancellation"))
    }
}

#[test]
fn 运行时可在工具执行期间取消当前轮() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, BlockingCancelAwareTools, identity);
    let control = TurnControl::new(AbortSignal::new());
    let cancel_handle = control.clone();

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        cancel_handle.cancel();
    });

    let error = runtime
        .handle_turn_streaming_with_control("请执行", control, |_| {})
        .expect_err("取消后应结束当前轮");

    assert!(error.is_cancelled());
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry.event_name() == Some("tool_result_cancelled")
            || entry.event_name() == Some("tool_call_cancelled")
    }));
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry.event_name() == Some("turn_failed")
            && entry
                .event_data()
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .is_some_and(|message| message.contains("已取消"))
    }));
}

#[test]
fn 运行时在开始前取消时不会执行模型() {
    let identity = ModelIdentity::new("local", "recording", ModelDisposition::Balanced);
    let model = RecordingModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);
    let control = TurnControl::new(AbortSignal::new());
    control.cancel();

    let error = runtime
        .handle_turn_streaming_with_control("不要执行", control, |_| {})
        .expect_err("预取消应直接失败");

    assert!(error.is_cancelled());
    assert!(runtime.model.seen_requests.borrow().is_empty());
}

#[test]
fn 运行时可暴露独立_turn_control() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let runtime = AgentRuntime::new(StubModel, StubTools, identity);

    let control = runtime.turn_control();
    assert!(!control.abort_signal().is_aborted());
    control.cancel();
    assert!(control.abort_signal().is_aborted());
}

#[test]
fn 取消轮次会标记为_cancelled_outcome() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, BlockingCancelAwareTools, identity);
    let control = TurnControl::new(AbortSignal::new());
    let cancel_handle = control.clone();

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        cancel_handle.cancel();
    });

    let subscriber = runtime.subscribe();
    let _ = runtime
        .handle_turn_streaming_with_control("请执行", control, |_| {})
        .expect_err("取消后应结束当前轮");

    let events = runtime.collect_events(subscriber).expect("应读取事件");
    let last_turn = runtime
        .tape()
        .entries()
        .iter()
        .any(|entry| entry.event_name() == Some("turn_failed"));
    assert!(last_turn);
    let lifecycle = events
        .into_iter()
        .find_map(|event| match event {
            RuntimeEvent::TurnLifecycle { turn } => Some(turn),
            _ => None,
        })
        .expect("应发布 turn lifecycle");
    assert_eq!(lifecycle.outcome, TurnOutcome::Cancelled);
    assert!(lifecycle.blocks.iter().any(|block| matches!(
        block,
        crate::TurnBlock::Cancelled { message } if message.contains("已取消")
    )));
}

#[test]
fn 运行时工具失败事件保留失败结果() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(StubModel, FailingTools, identity);
    let subscriber = runtime.subscribe();

    let _ = runtime.handle_turn("你好").expect("工具失败应写入轮次而不是直接报错");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ToolInvocation {
            outcome: ToolInvocationOutcome::Failed { message },
            ..
        } if message.contains("工具执行失败")
    )));
}

#[test]
fn 运行时会记录用户与助手消息() {
    let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
    let mut runtime =
        AgentRuntime::new(StubModel, StubTools, identity).with_instructions("保持简洁");

    let output = runtime.handle_turn("你好").expect("运行成功");

    assert_eq!(output.assistant_text, "已收到：你好");
    assert_eq!(runtime.tape().entries().len(), 7);
    assert_eq!(runtime.tape().entries()[6].event_name(), Some("turn_completed"));
    assert_eq!(output.visible_tools.len(), 3);
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
    assert_eq!(runtime.tape().entries().len(), 14);
    assert_eq!(
        runtime.tape().entries()[0].as_message().map(|value| value.content.clone()),
        Some("第一轮".into())
    );
    assert_eq!(
        runtime.tape().entries()[7].as_message().map(|value| value.content.clone()),
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
fn 运行时不会在旧默认步数后强行中断整轮() {
    let identity = ModelIdentity::new("local", "many-steps", ModelDisposition::Balanced);
    let model = ManyToolRoundsModel::new();
    let mut runtime =
        AgentRuntime::new(model, StubTools, identity).with_max_tool_calls_per_turn(16);

    let output = runtime.handle_turn("继续执行").expect("不应被旧默认步数上限打断");

    assert_eq!(output.assistant_text, "超过旧默认步数后仍成功收尾");
    let requests = runtime.model.seen_requests.borrow();
    assert!(requests.len() >= 10);
}

#[test]
fn 运行时不会自动追加最大步数收尾消息() {
    let identity = ModelIdentity::new("local", "continue", ModelDisposition::Balanced);
    let model = ContinueAfterToolModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let output = runtime.handle_turn("开始").expect("应收尾成功");

    assert_eq!(output.assistant_text, "已根据工具结果继续回答");
    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.conversation.iter().all(|item| {
        item.as_message().is_none_or(|message| !message.content.contains("最大步骤数"))
    })));
}

#[test]
fn 运行时按_stop_reason_而非工具片段决定是否继续() {
    let identity = ModelIdentity::new("local", "stop-reason", ModelDisposition::Balanced);
    let model = StopReasonDrivenModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let output = runtime.handle_turn("开始").expect("应按 stop reason 继续后再收尾");

    assert_eq!(output.assistant_text, "按 stop reason 收尾");
    assert_eq!(runtime.model.seen_requests.borrow().len(), 2);
}

#[test]
fn 工具片段与_stop_reason_不一致时会报错() {
    struct MismatchModel;

    impl LanguageModel for MismatchModel {
        type Error = CoreError;

        fn complete(&self, _request: CompletionRequest) -> Result<Completion, Self::Error> {
            Ok(Completion {
                segments: vec![CompletionSegment::ToolUse(ToolCall::new("search"))],
                stop_reason: CompletionStopReason::Stop,
                usage: None,
                response_body: None,
                http_status_code: None,
            })
        }
    }

    let identity = ModelIdentity::new("local", "mismatch", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(MismatchModel, StubTools, identity);

    let error = runtime.handle_turn("开始").expect_err("停止原因不匹配应失败");

    assert!(error.to_string().contains("停止原因与完成内容不匹配"));
    assert!(runtime.tape().entries().iter().all(|entry| entry.as_tool_call().is_none()));
    assert!(runtime.tape().entries().iter().all(|entry| entry.as_tool_result().is_none()));
}

#[test]
fn 未设置自定义指令时不会自动注入预算提示词() {
    let identity = ModelIdentity::new("local", "continue", ModelDisposition::Balanced);
    let model = ContinueAfterToolModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let _ = runtime.handle_turn("开始").expect("应成功完成");

    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests[0].instructions, None);
}

#[test]
fn 运行时会把模型输出上限映射为本次请求预算() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(200_000), output: Some(131_072) }));
    let model = BudgetRecordingModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let _ = runtime.handle_turn("开始").expect("应成功完成");

    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests[0].max_output_tokens, Some(131_072));
}

#[test]
fn 上下文接近窗口上限时会自动收紧输出预算() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(128), output: Some(64) }));
    let model = BudgetRecordingModel::new();
    let mut tape = SessionTape::new();
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({
            "status": "ok",
            "usage": {
                "input_tokens": 120,
                "output_tokens": 4,
                "total_tokens": 124,
                "cached_tokens": 0
            }
        })),
    ));
    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let _ = runtime.handle_turn("继续").expect("应成功完成");

    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests[0].max_output_tokens, Some(1));
}

#[test]
fn 可通过_builder_限制单轮最大工具调用次数() {
    let identity = ModelIdentity::new("local", "duplicate", ModelDisposition::Balanced);
    let model = DuplicateToolLoopModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity).with_max_tool_calls_per_turn(1);

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
    let _ = runtime.handoff("handoff", json!({"summary": "切到实现阶段"}));
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
    let _ = runtime.handoff("handoff", json!({"summary": "切到实现阶段"}));
    runtime.handle_turn("第二轮").expect("第二轮成功");

    let requests = runtime.model.seen_requests.borrow();
    let last_request = requests.last().expect("应记录第二轮请求");

    assert!(matches!(
        &last_request.conversation[0],
        ConversationItem::Message(message)
            if message.role == Role::User
                && message.content.contains("[context summary]")
                && message.content.contains("切到实现阶段")
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
fn responses_下一轮请求会重放结构化上下文() {
    let identity = ModelIdentity::new("openai", "responses", ModelDisposition::Balanced);
    let model = RequestRecordingModel::new();
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
    assert_eq!(requests[1].conversation.len(), 3);
    assert!(matches!(
        &requests[1].conversation[2],
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
                usage: None,
                failure_message: None,
                outcome: TurnOutcome::Succeeded,
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
                    started_at_ms,
                    finished_at_ms,
                    outcome: ToolInvocationOutcome::Succeeded { result },
                    ..
                } if started_at_ms <= finished_at_ms
                    && result.invocation_id == call.invocation_id
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
                usage: None,
                failure_message: None,
                outcome: TurnOutcome::Succeeded,
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
                    started_at_ms,
                    finished_at_ms,
                    outcome: ToolInvocationOutcome::Failed { message },
                    ..
                } if started_at_ms <= finished_at_ms
                    && call.tool_name == "search"
                    && message.contains("工具结果不匹配")
            )
    )));
}

#[test]
fn 成功轮会保留模型返回的真实_usage() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced);
    let mut runtime = AgentRuntime::new(UsageModel, StubTools, identity);
    let subscriber = runtime.subscribe();

    let output = runtime.handle_turn("统计这次 token").expect("运行成功");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert_eq!(
        output.completion.usage,
        Some(CompletionUsage {
            input_tokens: 21,
            output_tokens: 9,
            total_tokens: 30,
            cached_tokens: 0,
        })
    );
    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::TurnLifecycle {
            turn: TurnLifecycle {
                usage: Some(CompletionUsage {
                    input_tokens: 21,
                    output_tokens: 9,
                    total_tokens: 30,
                    ..
                }),
                ..
            }
        }
    )));
    let completion_event = runtime
        .tape()
        .entries()
        .iter()
        .find(|entry| entry.kind == "event" && entry.event_name() == Some("turn_completed"))
        .expect("应有完成事件");
    assert_eq!(
        completion_event
            .event_data()
            .and_then(|value| value.get("usage"))
            .and_then(|value| serde_json::from_value::<CompletionUsage>(value.clone()).ok()),
        Some(CompletionUsage {
            input_tokens: 21,
            output_tokens: 9,
            total_tokens: 30,
            cached_tokens: 0,
        })
    );
}

// --- Context compression tests ---

struct SummarizerModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl SummarizerModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for SummarizerModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        self.seen_requests.borrow_mut().push(request.clone());
        // If instructions contain "Summarize", this is a compression call
        if request.instructions.as_ref().is_some_and(|i| i.contains("handoff summary")) {
            return Ok(Completion::text("摘要：对话进行了多轮测试交互。"));
        }
        Ok(Completion::text("记录完成"))
    }
}

struct ContextLengthErrorModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
    fail_count: RefCell<usize>,
    max_failures: usize,
}

impl ContextLengthErrorModel {
    fn new(max_failures: usize) -> Self {
        Self { seen_requests: RefCell::new(Vec::new()), fail_count: RefCell::new(0), max_failures }
    }
}

impl LanguageModel for ContextLengthErrorModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        // Compression calls always succeed
        if request.instructions.as_ref().is_some_and(|i| i.contains("handoff summary")) {
            return Ok(Completion::text("压缩摘要：之前讨论了文件编辑。"));
        }

        self.seen_requests.borrow_mut().push(request);
        let count = *self.fail_count.borrow();
        if count < self.max_failures {
            *self.fail_count.borrow_mut() += 1;
            Err(CoreError::new("context_length_exceeded: max 128000 tokens, got 150000"))
        } else {
            Ok(Completion::text("压缩后成功"))
        }
    }
}

#[test]
fn 上下文未超阈值时不触发压缩() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(200_000), output: Some(8192) }));
    let model = SummarizerModel::new();
    let mut runtime =
        AgentRuntime::new(model, StubTools, identity).with_context_pressure_threshold(0.80);

    let _ = runtime.handle_turn("你好").expect("应成功完成");

    // No compression call should have been made — only the regular turn call
    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].instructions.as_ref().is_none_or(|i| !i.contains("handoff summary")));
    // No compression anchor
    assert!(runtime.tape().anchors().iter().all(|a| a.name != "context_compression"));
}

#[test]
fn 上下文超阈值时触发压缩生成锚点() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(32), output: Some(16) }));
    let model = SummarizerModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "这是一段很长的历史消息用来填充上下文窗口。"));
    tape.append(Message::new(Role::Assistant, "这是一段很长的历史回答用来填充上下文窗口。"));
    tape.append(Message::new(Role::User, "第二轮历史消息。"));
    tape.append(Message::new(Role::Assistant, "第二轮历史回答。"));
    // Simulate previous turn's real token usage exceeding threshold
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({"status": "ok", "usage": {"input_tokens": 28, "output_tokens": 4, "total_tokens": 32}})),
    ));

    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape)
        .with_context_pressure_threshold(0.50);

    let _ = runtime.handle_turn("继续").expect("应成功完成");

    // A compression anchor should have been created
    assert!(runtime.tape().anchors().iter().any(|a| a.name == "context_compression"));
}

#[test]
fn 压缩锚点仅保留摘要字段() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(32), output: Some(16) }));
    let model = SummarizerModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "历史消息一"));
    tape.append(Message::new(Role::Assistant, "历史回答一"));
    tape.append(Message::new(Role::User, "历史消息二"));
    tape.append(Message::new(Role::Assistant, "历史回答二"));
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({"status": "ok", "usage": {"input_tokens": 28, "output_tokens": 4, "total_tokens": 32}})),
    ));

    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape)
        .with_context_pressure_threshold(0.50);

    let _ = runtime.handle_turn("继续").expect("应成功完成");

    let anchor = runtime
        .tape()
        .anchors()
        .into_iter()
        .find(|anchor| anchor.name == "context_compression")
        .expect("应创建压缩锚点");
    let summary =
        anchor.state.get("summary").and_then(|value| value.as_str()).expect("压缩锚点应包含摘要");

    assert!(summary.contains("摘要"));
    // No legacy metadata fields
    assert!(anchor.state.get("source_entry_ids").is_none());
    assert!(anchor.state.get("owner").is_none());
    assert!(anchor.state.get("phase").is_none());
    assert!(anchor.state.get("next_steps").is_none());
}

#[test]
fn 压缩后_default_view_从新锚点开始() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(32), output: Some(16) }));
    let model = SummarizerModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "历史消息一"));
    tape.append(Message::new(Role::Assistant, "历史回答一"));
    tape.append(Message::new(Role::User, "历史消息二"));
    tape.append(Message::new(Role::Assistant, "历史回答二"));
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({"status": "ok", "usage": {"input_tokens": 28, "output_tokens": 4, "total_tokens": 32}})),
    ));

    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape)
        .with_context_pressure_threshold(0.50);

    let _ = runtime.handle_turn("新消息").expect("应成功完成");

    let view = runtime.tape().default_view();
    // The view should start from the compression anchor, not include old messages
    assert!(view.origin_anchor.as_ref().is_some_and(|a| a.name == "context_compression"));
    // Old messages ("历史消息一") should not appear in the view
    assert!(view.messages.iter().all(|m| m.content != "历史消息一"));
}

#[test]
fn 模型返回_context_length_exceeded_时压缩并重试() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(200_000), output: Some(8192) }));
    // First call fails with context error, after compression the second call succeeds
    let model = ContextLengthErrorModel::new(1);
    let mut tape = SessionTape::new();
    // Need enough entries for compress_context() to proceed (>= 4 conversation items)
    tape.append(Message::new(Role::User, "历史消息一"));
    tape.append(Message::new(Role::Assistant, "历史回答一"));
    tape.append(Message::new(Role::User, "历史消息二"));
    tape.append(Message::new(Role::Assistant, "历史回答二"));
    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let output = runtime.handle_turn("你好").expect("压缩重试后应成功");

    assert_eq!(output.assistant_text, "压缩后成功");
    assert!(runtime.tape().anchors().iter().any(|a| a.name == "context_compression"));
}

#[test]
fn 压缩重试最多一次() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(200_000), output: Some(8192) }));
    // Fail twice — first triggers compress+retry, second should propagate error
    let model = ContextLengthErrorModel::new(2);
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "历史消息一"));
    tape.append(Message::new(Role::Assistant, "历史回答一"));
    tape.append(Message::new(Role::User, "历史消息二"));
    tape.append(Message::new(Role::Assistant, "历史回答二"));
    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let error = runtime.handle_turn("你好").expect_err("第二次失败应传播");

    assert!(error.to_string().contains("模型执行失败"));
    assert!(error.to_string().contains("context_length_exceeded"));
}

#[test]
fn context_stats_返回当前请求视角的压力比值() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(1000), output: Some(500) }));
    let model = RecordingModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "测试消息"));
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({
            "status": "ok",
            "usage": {
                "input_tokens": 320,
                "output_tokens": 8,
                "total_tokens": 328,
                "cached_tokens": 0
            }
        })),
    ));
    let runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let stats = runtime.context_stats();

    assert_eq!(stats.total_entries, 2);
    assert_eq!(stats.anchor_count, 0);
    assert_eq!(stats.context_limit, Some(1000));
    assert_eq!(stats.output_limit, Some(500));
    assert!(stats.pressure_ratio.is_some());
    assert_eq!(stats.last_input_tokens, Some(320));
    let expected_ratio = 320.0 / 1000.0;
    assert!((stats.pressure_ratio.unwrap() - expected_ratio).abs() < f64::EPSILON);
}

#[test]
fn 锚点收缩上下文后_context_stats_会清空旧_token_统计() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(1000), output: Some(500) }));
    let model = RecordingModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "很长的旧历史消息一，很长的旧历史消息一。"));
    tape.append(Message::new(Role::Assistant, "很长的旧历史回答一，很长的旧历史回答一。"));
    tape.append(Message::new(Role::User, "很长的旧历史消息二，很长的旧历史消息二。"));
    tape.append(Message::new(Role::Assistant, "很长的旧历史回答二，很长的旧历史回答二。"));
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({"status": "ok", "usage": {"input_tokens": 900, "output_tokens": 64, "total_tokens": 964}})),
    ));
    tape.handoff("context_compression", json!({"summary": "摘要：旧历史已经压缩。"}));
    tape.append(Message::new(Role::User, "新历史一"));
    tape.append(Message::new(Role::Assistant, "新回答一"));
    tape.append(Message::new(Role::User, "新历史二"));
    tape.append(Message::new(Role::Assistant, "新回答二"));
    let runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let stats = runtime.context_stats();

    assert_eq!(stats.last_input_tokens, None);
    assert_eq!(stats.pressure_ratio, None);
}

#[test]
fn 锚点收缩上下文后不会因旧_token_统计重复压缩() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(1200), output: Some(500) }));
    let model = SummarizerModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "很长的旧历史消息一，很长的旧历史消息一。"));
    tape.append(Message::new(Role::Assistant, "很长的旧历史回答一，很长的旧历史回答一。"));
    tape.append(Message::new(Role::User, "很长的旧历史消息二，很长的旧历史消息二。"));
    tape.append(Message::new(Role::Assistant, "很长的旧历史回答二，很长的旧历史回答二。"));
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({"status": "ok", "usage": {"input_tokens": 900, "output_tokens": 64, "total_tokens": 964}})),
    ));
    tape.handoff("context_compression", json!({"summary": "摘要：旧历史已经压缩。"}));
    tape.append(Message::new(Role::User, "新历史一"));
    tape.append(Message::new(Role::Assistant, "新回答一"));
    tape.append(Message::new(Role::User, "新历史二"));
    tape.append(Message::new(Role::Assistant, "新回答二"));

    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape)
        .with_context_pressure_threshold(0.60);
    let pre_turn_stats = runtime.context_stats();
    assert_eq!(pre_turn_stats.pressure_ratio, None);

    let _ = runtime.handle_turn("继续").expect("应成功完成");

    let requests = runtime.model.seen_requests.borrow();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].instructions.as_ref().is_none_or(|i| !i.contains("handoff summary")));
}

#[test]
fn 压缩事件会被发布到事件流() {
    let identity = ModelIdentity::new("openai", "gpt-4.1", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(32), output: Some(16) }));
    let model = SummarizerModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "填充历史消息用来触发压力检测。"));
    tape.append(Message::new(Role::Assistant, "填充历史回答用来触发压力检测。"));
    tape.append(Message::new(Role::User, "第二轮历史消息。"));
    tape.append(Message::new(Role::Assistant, "第二轮历史回答。"));
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(json!({"status": "ok", "usage": {"input_tokens": 28, "output_tokens": 4, "total_tokens": 32}})),
    ));

    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape)
        .with_context_pressure_threshold(0.50);
    let subscriber = runtime.subscribe();

    let _ = runtime.handle_turn("继续").expect("应成功完成");
    let events = runtime.collect_events(subscriber).expect("读取事件成功");

    assert!(events.iter().any(|event| matches!(
        event,
        RuntimeEvent::ContextCompressed { summary } if summary.contains("摘要")
    )));
}

// --- Tape tools tests ---

struct TapeInfoModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl TapeInfoModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for TapeInfoModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let step = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request.clone());
        if step == 0 {
            Ok(Completion {
                segments: vec![CompletionSegment::ToolUse(ToolCall::new("tape_info"))],
                stop_reason: CompletionStopReason::ToolUse,
                usage: None,
                response_body: None,
                http_status_code: None,
            })
        } else {
            let saw_info = request.conversation.iter().any(|item| {
                item.as_tool_result().is_some_and(|result| result.content.contains("entries:"))
            });
            if saw_info {
                Ok(Completion::text("已获取上下文统计信息"))
            } else {
                Err(CoreError::new("未看到 tape_info 结果"))
            }
        }
    }
}

struct TapeHandoffModel {
    seen_requests: RefCell<Vec<CompletionRequest>>,
}

impl TapeHandoffModel {
    fn new() -> Self {
        Self { seen_requests: RefCell::new(Vec::new()) }
    }
}

impl LanguageModel for TapeHandoffModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let step = self.seen_requests.borrow().len();
        self.seen_requests.borrow_mut().push(request.clone());
        if step == 0 {
            Ok(Completion {
                segments: vec![CompletionSegment::ToolUse(
                    ToolCall::new("tape_handoff").with_arguments_value(
                        json!({"summary": "测试摘要：对话进行了多轮交互", "name": "test_anchor"}),
                    ),
                )],
                stop_reason: CompletionStopReason::ToolUse,
                usage: None,
                response_body: None,
                http_status_code: None,
            })
        } else {
            // After handoff, the tool_call is before the anchor and gets truncated.
            // The tool_result becomes orphaned and is filtered out.
            // Instead, we should see the context summary injected from the anchor.
            let saw_summary = request.conversation.iter().any(|item| {
                item.as_message().is_some_and(|msg| msg.content.contains("[context summary]"))
            });
            if saw_summary {
                Ok(Completion::text("已创建锚点"))
            } else {
                // Fallback: the tool_result might still be visible if not orphaned
                let saw_anchor = request.conversation.iter().any(|item| {
                    item.as_tool_result()
                        .is_some_and(|result| result.content.contains("anchor added"))
                });
                if saw_anchor {
                    Ok(Completion::text("已创建锚点"))
                } else {
                    Err(CoreError::new("未看到 tape_handoff 结果或 context summary"))
                }
            }
        }
    }
}

#[test]
fn tape_info_工具返回上下文统计() {
    let identity = ModelIdentity::new("local", "tape-info", ModelDisposition::Balanced)
        .with_limit(Some(agent_core::ModelLimit { context: Some(100_000), output: Some(8192) }));
    let model = TapeInfoModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    let output = runtime.handle_turn("查看上下文状态").expect("应成功完成");

    assert_eq!(output.assistant_text, "已获取上下文统计信息");
    // Verify the tool result was recorded
    assert!(runtime.tape().entries().iter().any(|entry| {
        entry.as_tool_result().is_some_and(|result| {
            result.tool_name == "tape_info" && result.content.contains("entries:")
        })
    }));
}

#[test]
fn tape_handoff_工具创建锚点() {
    let identity = ModelIdentity::new("local", "tape-handoff", ModelDisposition::Balanced);
    let model = TapeHandoffModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "历史消息"));
    tape.append(Message::new(Role::Assistant, "历史回答"));
    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let output = runtime.handle_turn("创建检查点").expect("应成功完成");

    assert_eq!(output.assistant_text, "已创建锚点");
    assert!(runtime.tape().anchors().iter().any(|a| a.name == "test_anchor"));
    let anchor =
        runtime.tape().anchors().into_iter().find(|a| a.name == "test_anchor").expect("应创建锚点");
    assert_eq!(
        anchor.state.get("summary").and_then(|v| v.as_str()),
        Some("测试摘要：对话进行了多轮交互")
    );
}

#[test]
fn runtime_tool_bridge_创建锚点后后续请求会过滤孤立_tool_result() {
    let identity = ModelIdentity::new("local", "tape-handoff", ModelDisposition::Balanced);
    let model = TapeHandoffModel::new();
    let mut tape = SessionTape::new();
    tape.append(Message::new(Role::User, "历史消息"));
    tape.append(Message::new(Role::Assistant, "历史回答"));
    let mut runtime = AgentRuntime::with_tape(model, StubTools, identity, tape);

    let output = runtime.handle_turn("创建检查点").expect("应成功完成");

    assert_eq!(output.assistant_text, "已创建锚点");
    let entries = runtime.tape().entries();
    let tool_result_index = entries
        .iter()
        .position(|entry| {
            entry.as_tool_result().is_some_and(|result| result.tool_name == "tape_handoff")
        })
        .expect("应记录 runtime tool 结果");
    let anchor_index = entries
        .iter()
        .position(|entry| entry.anchor_name() == Some("test_anchor"))
        .expect("应创建 test_anchor");
    let request = runtime.build_completion_request("turn-check", "completion", 0);

    assert!(tool_result_index > anchor_index);
    assert!(request.conversation.iter().all(|item| {
        item.as_tool_result().is_none_or(|result| result.tool_name != "tape_handoff")
    }));
}

#[test]
fn 孤立的_tool_result_会被过滤() {
    use super::request::drop_orphaned_tool_results;

    let call = ToolCall::new("search").with_invocation_id("call-1");
    let matching_result = agent_core::ToolResult::from_call(&call, "结果");
    let orphan_result = agent_core::ToolResult {
        invocation_id: "orphan-id".into(),
        tool_name: "search".into(),
        content: "孤立结果".into(),
        response_id: None,
        details: None,
    };

    let conversation = vec![
        ConversationItem::ToolResult(orphan_result),
        ConversationItem::ToolCall(call),
        ConversationItem::ToolResult(matching_result),
        ConversationItem::Message(Message::new(Role::User, "用户消息")),
    ];

    let filtered = drop_orphaned_tool_results(conversation);

    assert_eq!(filtered.len(), 3);
    // The orphan (invocation_id = "orphan-id") should be gone
    assert!(
        filtered
            .iter()
            .all(|item| { item.as_tool_result().is_none_or(|r| r.invocation_id != "orphan-id") })
    );
    // The matching result should remain
    assert!(
        filtered
            .iter()
            .any(|item| { item.as_tool_result().is_some_and(|r| r.invocation_id == "call-1") })
    );
}

#[test]
fn 无锚点摘要时不注入上下文消息() {
    let identity = ModelIdentity::new("local", "recording", ModelDisposition::Balanced);
    let model = RecordingModel::new();
    let mut runtime = AgentRuntime::new(model, StubTools, identity);

    runtime.handle_turn("第一轮").expect("第一轮成功");
    // Handoff with empty summary
    let _ = runtime.handoff("handoff", json!({"summary": ""}));
    runtime.handle_turn("第二轮").expect("第二轮成功");

    let requests = runtime.model.seen_requests.borrow();
    let last_request = requests.last().expect("应记录第二轮请求");

    // First item should be user message, not a context summary
    assert!(matches!(
        &last_request.conversation[0],
        ConversationItem::Message(message) if message.content == "第二轮"
    ));
}
