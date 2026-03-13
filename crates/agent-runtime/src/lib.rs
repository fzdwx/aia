use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{
    AbortSignal, Completion, CompletionRequest, CompletionSegment, LanguageModel, Message,
    ModelIdentity, Role, StreamEvent, ToolCall, ToolDefinition, ToolExecutionContext, ToolExecutor,
    ToolOutputDelta, ToolResult,
};
use serde_json::json;
use session_tape::{Anchor, Handoff, SessionTape, TapeEntry};

pub struct AgentRuntime<M, T> {
    model: M,
    tools: T,
    tape: SessionTape,
    model_identity: ModelIdentity,
    instructions: Option<String>,
    disabled_tools: BTreeSet<String>,
    workspace_root: Option<std::path::PathBuf>,
    events: Vec<RuntimeEvent>,
    subscribers: BTreeMap<RuntimeSubscriberId, usize>,
    next_subscriber_id: RuntimeSubscriberId,
}

pub type RuntimeSubscriberId = u64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolInvocationLifecycle {
    pub call: ToolCall,
    pub outcome: ToolInvocationOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnLifecycle {
    pub turn_id: String,
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
    pub source_entry_ids: Vec<u64>,
    pub user_message: String,
    pub assistant_message: Option<String>,
    pub thinking: Option<String>,
    pub tool_invocations: Vec<ToolInvocationLifecycle>,
    pub failure_message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolInvocationOutcome {
    Succeeded { result: ToolResult },
    Failed { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEvent {
    UserMessage { content: String },
    AssistantMessage { content: String },
    ToolInvocation { call: ToolCall, outcome: ToolInvocationOutcome },
    TurnLifecycle { turn: TurnLifecycle },
    TurnFailed { message: String },
}

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub fn new(model: M, tools: T, model_identity: ModelIdentity) -> Self {
        Self::with_tape(model, tools, model_identity, SessionTape::new())
    }

    pub fn with_tape(model: M, tools: T, model_identity: ModelIdentity, tape: SessionTape) -> Self {
        Self {
            model,
            tools,
            tape,
            model_identity,
            instructions: None,
            disabled_tools: BTreeSet::new(),
            workspace_root: None,
            events: Vec::new(),
            subscribers: BTreeMap::new(),
            next_subscriber_id: 1,
        }
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    pub fn with_workspace_root(mut self, workspace_root: impl Into<std::path::PathBuf>) -> Self {
        self.workspace_root = Some(workspace_root.into());
        self
    }

    pub fn handle_turn(
        &mut self,
        user_input: impl Into<String>,
    ) -> Result<TurnOutput, RuntimeError> {
        self.handle_turn_streaming(user_input, |_| {})
    }

    pub fn handle_turn_streaming(
        &mut self,
        user_input: impl Into<String>,
        mut on_delta: impl FnMut(StreamEvent),
    ) -> Result<TurnOutput, RuntimeError> {
        let turn_id = next_turn_id();
        let started_at_ms = now_timestamp_ms();
        let user_message = Message::new(Role::User, user_input.into());
        let user_entry_id =
            self.tape.append_entry(TapeEntry::message(&user_message).with_run_id(&turn_id));
        let mut source_entry_ids = vec![user_entry_id];
        self.publish_event(RuntimeEvent::UserMessage { content: user_message.content.clone() });
        let view = self.tape.default_view();
        let mut conversation = Vec::new();
        if let Some(anchor) = view.origin_anchor.as_ref() {
            conversation.push(anchor_state_message(anchor));
        }
        conversation.extend(view.messages);

        let request = CompletionRequest {
            model: self.model_identity.clone(),
            instructions: self.instructions.clone(),
            conversation,
            available_tools: self.visible_tools(),
        };

        let completion = match self.model.complete_streaming(request, &mut on_delta) {
            Ok(completion) => completion,
            Err(error) => {
                let runtime_error = RuntimeError::model(error);
                let failure_event_id = self.tape.append_entry(
                    TapeEntry::event(
                        "turn_failed",
                        Some(json!({"message": runtime_error.to_string()})),
                    )
                    .with_run_id(&turn_id)
                    .with_meta("source_entry_ids", json!([user_entry_id])),
                );
                source_entry_ids.push(failure_event_id);
                self.publish_event(RuntimeEvent::TurnFailed { message: runtime_error.to_string() });
                self.publish_turn_lifecycle(TurnLifecycle {
                    turn_id,
                    started_at_ms,
                    finished_at_ms: now_timestamp_ms(),
                    source_entry_ids,
                    user_message: user_message.content.clone(),
                    assistant_message: None,
                    thinking: None,
                    tool_invocations: Vec::new(),
                    failure_message: Some(runtime_error.to_string()),
                });
                return Err(runtime_error);
            }
        };

        // Extract and persist thinking content
        let thinking = completion.thinking_text();
        if let Some(ref thinking_text) = thinking {
            let thinking_entry_id =
                self.tape.append_entry(TapeEntry::thinking(thinking_text).with_run_id(&turn_id));
            source_entry_ids.push(thinking_entry_id);
        }

        let assistant_text = completion.plain_text();
        let assistant_message = Message::new(Role::Assistant, assistant_text.clone());
        let assistant_entry_id =
            self.tape.append_entry(TapeEntry::message(&assistant_message).with_run_id(&turn_id));
        source_entry_ids.push(assistant_entry_id);
        self.publish_event(RuntimeEvent::AssistantMessage { content: assistant_text.clone() });
        let mut tool_invocations = Vec::new();
        let available_tool_names = self
            .visible_tools()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();
        for segment in &completion.segments {
            if let CompletionSegment::ToolUse(call) = segment {
                let tool_call_entry_id =
                    self.tape.append_entry(TapeEntry::tool_call(call).with_run_id(&turn_id));
                source_entry_ids.push(tool_call_entry_id);
                if !available_tool_names.contains(&call.tool_name) {
                    let runtime_error = RuntimeError::tool_unavailable(call.tool_name.clone());
                    let reject_event_id = self.tape.append_entry(
                        TapeEntry::event(
                            "tool_call_rejected",
                            Some(json!({"message": runtime_error.to_string()})),
                        )
                        .with_run_id(&turn_id)
                        .with_meta(
                            "source_entry_ids",
                            json!([assistant_entry_id, tool_call_entry_id]),
                        ),
                    );
                    source_entry_ids.push(reject_event_id);
                    self.publish_event(RuntimeEvent::ToolInvocation {
                        call: call.clone(),
                        outcome: ToolInvocationOutcome::Failed {
                            message: runtime_error.to_string(),
                        },
                    });
                    tool_invocations.push(ToolInvocationLifecycle {
                        call: call.clone(),
                        outcome: ToolInvocationOutcome::Failed {
                            message: runtime_error.to_string(),
                        },
                    });
                    self.publish_turn_lifecycle(TurnLifecycle {
                        turn_id: turn_id.clone(),
                        started_at_ms,
                        finished_at_ms: now_timestamp_ms(),
                        source_entry_ids,
                        user_message: user_message.content.clone(),
                        assistant_message: Some(assistant_text.clone()),
                        thinking: thinking.clone(),
                        tool_invocations,
                        failure_message: Some(runtime_error.to_string()),
                    });
                    return Err(runtime_error);
                }

                match self.tools.call(
                    call,
                    &mut |delta: ToolOutputDelta| {
                        on_delta(StreamEvent::ToolOutputDelta {
                            invocation_id: call.invocation_id.clone(),
                            stream: delta.stream,
                            text: delta.text,
                        });
                    },
                    &ToolExecutionContext {
                        run_id: turn_id.clone(),
                        workspace_root: self.workspace_root.clone(),
                        abort: AbortSignal::new(),
                    },
                ) {
                    Ok(result) => {
                        if result.invocation_id != call.invocation_id
                            || result.tool_name != call.tool_name
                        {
                            let runtime_error = RuntimeError::tool_result_mismatch(call, &result);
                            let reject_event_id = self.tape.append_entry(
                                TapeEntry::event(
                                    "tool_result_rejected",
                                    Some(json!({"message": runtime_error.to_string()})),
                                )
                                .with_run_id(&turn_id)
                                .with_meta(
                                    "source_entry_ids",
                                    json!([assistant_entry_id, tool_call_entry_id]),
                                ),
                            );
                            source_entry_ids.push(reject_event_id);
                            self.publish_event(RuntimeEvent::ToolInvocation {
                                call: call.clone(),
                                outcome: ToolInvocationOutcome::Failed {
                                    message: runtime_error.to_string(),
                                },
                            });
                            tool_invocations.push(ToolInvocationLifecycle {
                                call: call.clone(),
                                outcome: ToolInvocationOutcome::Failed {
                                    message: runtime_error.to_string(),
                                },
                            });
                            self.publish_turn_lifecycle(TurnLifecycle {
                                turn_id: turn_id.clone(),
                                started_at_ms,
                                finished_at_ms: now_timestamp_ms(),
                                source_entry_ids,
                                user_message: user_message.content.clone(),
                                assistant_message: Some(assistant_text.clone()),
                                thinking: thinking.clone(),
                                tool_invocations,
                                failure_message: Some(runtime_error.to_string()),
                            });
                            return Err(runtime_error);
                        }

                        let tool_result_entry_id = self
                            .tape
                            .append_entry(TapeEntry::tool_result(&result).with_run_id(&turn_id));
                        source_entry_ids.push(tool_result_entry_id);
                        let tool_result_event_id = self.tape.append_entry(
                            TapeEntry::event(
                                "tool_result_recorded",
                                Some(json!({"tool_name": result.tool_name.clone()})),
                            )
                            .with_run_id(&turn_id)
                            .with_meta(
                                "source_entry_ids",
                                json!([
                                    assistant_entry_id,
                                    tool_call_entry_id,
                                    tool_result_entry_id
                                ]),
                            ),
                        );
                        source_entry_ids.push(tool_result_event_id);
                        self.publish_event(RuntimeEvent::ToolInvocation {
                            call: call.clone(),
                            outcome: ToolInvocationOutcome::Succeeded { result: result.clone() },
                        });
                        tool_invocations.push(ToolInvocationLifecycle {
                            call: call.clone(),
                            outcome: ToolInvocationOutcome::Succeeded { result },
                        });
                    }
                    Err(error) => {
                        let runtime_error = RuntimeError::tool(error);
                        let failure_event_id = self.tape.append_entry(
                            TapeEntry::event(
                                "tool_call_failed",
                                Some(json!({"message": runtime_error.to_string()})),
                            )
                            .with_run_id(&turn_id)
                            .with_meta(
                                "source_entry_ids",
                                json!([assistant_entry_id, tool_call_entry_id]),
                            ),
                        );
                        source_entry_ids.push(failure_event_id);
                        self.publish_event(RuntimeEvent::ToolInvocation {
                            call: call.clone(),
                            outcome: ToolInvocationOutcome::Failed {
                                message: runtime_error.to_string(),
                            },
                        });
                        tool_invocations.push(ToolInvocationLifecycle {
                            call: call.clone(),
                            outcome: ToolInvocationOutcome::Failed {
                                message: runtime_error.to_string(),
                            },
                        });
                        self.publish_turn_lifecycle(TurnLifecycle {
                            turn_id: turn_id.clone(),
                            started_at_ms,
                            finished_at_ms: now_timestamp_ms(),
                            source_entry_ids,
                            user_message: user_message.content.clone(),
                            assistant_message: Some(assistant_text.clone()),
                            thinking: thinking.clone(),
                            tool_invocations,
                            failure_message: Some(runtime_error.to_string()),
                        });
                        return Err(runtime_error);
                    }
                }
            }
        }

        self.publish_turn_lifecycle(TurnLifecycle {
            turn_id,
            started_at_ms,
            finished_at_ms: now_timestamp_ms(),
            source_entry_ids,
            user_message: user_message.content,
            assistant_message: Some(assistant_text.clone()),
            thinking,
            tool_invocations,
            failure_message: None,
        });

        Ok(TurnOutput { assistant_text, completion, visible_tools: self.visible_tools() })
    }

    pub fn disable_tool(&mut self, tool_name: impl Into<String>) {
        self.disabled_tools.insert(tool_name.into());
    }

    pub fn enable_tool(&mut self, tool_name: &str) {
        self.disabled_tools.remove(tool_name);
    }

    pub fn visible_tools(&self) -> Vec<ToolDefinition> {
        self.tools
            .definitions()
            .into_iter()
            .filter(|definition| !self.disabled_tools.contains(&definition.name))
            .collect()
    }

    pub fn handoff(&mut self, name: impl Into<String>, state: serde_json::Value) -> Handoff {
        self.tape.handoff(name, state)
    }

    pub fn tape(&self) -> &SessionTape {
        &self.tape
    }

    pub fn subscribe(&mut self) -> RuntimeSubscriberId {
        let subscriber_id = self.next_subscriber_id;
        self.next_subscriber_id += 1;
        self.subscribers.insert(subscriber_id, self.events.len());
        subscriber_id
    }

    pub fn collect_events(
        &mut self,
        subscriber_id: RuntimeSubscriberId,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let cursor = self
            .subscribers
            .get_mut(&subscriber_id)
            .ok_or_else(|| RuntimeError::subscription(format!("订阅者不存在：{subscriber_id}")))?;
        let events = self.events[*cursor..].to_vec();
        *cursor = self.events.len();
        Ok(events)
    }

    fn publish_event(&mut self, event: RuntimeEvent) {
        self.events.push(event);
    }

    fn publish_turn_lifecycle(&mut self, turn: TurnLifecycle) {
        self.publish_event(RuntimeEvent::TurnLifecycle { turn });
    }
}

fn next_turn_id() -> String {
    static NEXT_TURN_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_TURN_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时间应晚于 UNIX_EPOCH")
        .as_millis();
    format!("turn-{now_ms}-{id}")
}

fn now_timestamp_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("系统时间应晚于 UNIX_EPOCH").as_millis()
}

fn anchor_state_message(anchor: &Anchor) -> Message {
    let state = &anchor.state;
    let phase = state.get("phase").and_then(|v| v.as_str()).unwrap_or(&anchor.name);
    let summary = state.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let next_steps = state
        .get("next_steps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("、"))
        .unwrap_or_default();
    let source_entry_ids = state
        .get("source_entry_ids")
        .and_then(|v| v.as_array())
        .map(|arr| format!("{:?}", arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>()))
        .unwrap_or_else(|| "[]".into());
    let owner = state.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    Message::new(
        Role::System,
        format!(
            "当前阶段: {}\n锚点摘要: {}\n下一步: {}\n来源条目: {}\n所有者: {}",
            phase, summary, next_steps, source_entry_ids, owner,
        ),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnOutput {
    pub assistant_text: String,
    pub completion: Completion,
    pub visible_tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeError {
    message: String,
}

impl RuntimeError {
    pub fn model(error: impl fmt::Display) -> Self {
        Self { message: format!("模型执行失败：{error}") }
    }

    pub fn subscription(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub fn tool(error: impl fmt::Display) -> Self {
        Self { message: format!("工具执行失败：{error}") }
    }

    pub fn tool_unavailable(tool_name: impl Into<String>) -> Self {
        Self { message: format!("工具不可用：{}", tool_name.into()) }
    }

    pub fn tool_result_mismatch(
        call: &agent_core::ToolCall,
        result: &agent_core::ToolResult,
    ) -> Self {
        Self {
            message: format!(
                "工具结果不匹配：调用 {}#{}, 结果 {}#{}",
                call.tool_name, call.invocation_id, result.tool_name, result.invocation_id
            ),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use agent_core::{
        Completion, CompletionRequest, CompletionSegment, CoreError, LanguageModel, Message,
        ModelDisposition, ModelIdentity, Role, ToolCall, ToolDefinition, ToolExecutionContext,
        ToolExecutor, ToolOutputDelta, ToolResult,
    };
    use serde_json::json;
    use session_tape::SessionTape;

    use super::{
        AgentRuntime, RuntimeEvent, ToolInvocationLifecycle, ToolInvocationOutcome, TurnLifecycle,
    };

    struct StubModel;

    impl LanguageModel for StubModel {
        type Error = CoreError;

        fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
            let latest = request
                .conversation
                .last()
                .map(|message| message.content.clone())
                .unwrap_or_else(|| "空输入".into());
            Ok(Completion {
                segments: vec![
                    CompletionSegment::Text(format!("已收到：{latest}")),
                    CompletionSegment::ToolUse(ToolCall::new("search")),
                ],
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

    impl LanguageModel for RecordingModel {
        type Error = CoreError;

        fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
            self.seen_requests.borrow_mut().push(request);
            Ok(Completion::text("记录完成"))
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
        assert_eq!(runtime.tape().entries().len(), 5);
        assert_eq!(output.visible_tools.len(), 1);
    }

    #[test]
    fn 运行时可生成交接() {
        let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
        let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);
        runtime.handle_turn("开始").expect("运行成功");

        let handoff = runtime
            .handoff("handoff", json!({"summary": "发现阶段结束", "next_steps": ["进入实现"]}));

        assert_eq!(
            handoff.anchor.state.get("summary").and_then(|v| v.as_str()),
            Some("发现阶段结束"),
        );
        assert_eq!(
            handoff.anchor.state.get("next_steps").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(1),
        );
        assert!(handoff.event_id > handoff.anchor.entry_id);
    }

    #[test]
    fn 已禁用工具不会暴露给模型() {
        let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
        let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);

        runtime.disable_tool("search");

        let error = runtime.handle_turn("你好").expect_err("应当失败");

        assert!(error.to_string().contains("工具不可用"));
    }

    #[test]
    fn 多轮调用会保留上下文() {
        let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
        let mut runtime = AgentRuntime::new(StubModel, StubTools, identity);

        runtime.handle_turn("第一轮").expect("第一轮成功");
        let output = runtime.handle_turn("第二轮").expect("第二轮成功");

        assert_eq!(output.assistant_text, "已收到：第二轮");
        assert_eq!(runtime.tape().entries().len(), 10);
        assert_eq!(
            runtime.tape().entries()[0].as_message().map(|value| value.content.clone()),
            Some("第一轮".into())
        );
        assert_eq!(
            runtime.tape().entries()[5].as_message().map(|value| value.content.clone()),
            Some("第二轮".into())
        );
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

        assert_eq!(default_messages.len(), 3);
        assert_eq!(default_messages[0].content, "第二轮");
        assert_eq!(default_messages[1].content, "已收到：第二轮");
        assert!(default_messages[2].content.starts_with("工具 search #tool-call-"));
        assert!(default_messages[2].content.ends_with("输出: 未实现"));
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

        assert_eq!(last_request.conversation[0].role, Role::System);
        assert!(last_request.conversation[0].content.contains("当前阶段: handoff"));
        assert!(last_request.conversation[0].content.contains("锚点摘要: 切到实现阶段"));
        assert_eq!(last_request.conversation[1].content, "第二轮");
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

        assert_eq!(last_request.conversation[0].content, "历史用户消息");
        assert_eq!(last_request.conversation[1].content, "历史助手消息");
        assert_eq!(last_request.conversation[2].content, "新的输入");
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
            events
                .iter()
                .filter(|event| matches!(event, RuntimeEvent::ToolInvocation { .. }))
                .count(),
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

        let error = runtime.handle_turn("你好").expect_err("应当因为工具被禁用而失败");
        let events = runtime.collect_events(subscriber).expect("读取事件成功");

        assert!(error.to_string().contains("工具不可用"));
        assert!(!runtime.tape().entries().iter().any(|entry| entry.as_tool_result().is_some()));
        assert!(events.iter().any(|event| matches!(
            event,
            RuntimeEvent::ToolInvocation { call, outcome: ToolInvocationOutcome::Failed { message } }
                if call.tool_name == "search" && message.contains("工具不可用")
        )));
    }

    #[test]
    fn 工具结果调用标识错配时会被拒绝() {
        let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
        let mut runtime = AgentRuntime::new(StubModel, MismatchedTools, identity);
        let subscriber = runtime.subscribe();

        let error = runtime.handle_turn("你好").expect_err("应当因结果错配失败");
        let events = runtime.collect_events(subscriber).expect("读取事件成功");

        assert!(error.to_string().contains("工具结果不匹配"));
        assert!(!runtime.tape().entries().iter().any(|entry| entry.as_tool_result().is_some()));
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
    fn 失败轮也会聚合成完整轮次块事件() {
        let identity = ModelIdentity::new("local", "stub", ModelDisposition::Balanced);
        let mut runtime = AgentRuntime::new(StubModel, MismatchedTools, identity);
        let subscriber = runtime.subscribe();

        let _ = runtime.handle_turn("你好");
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
                    assistant_message: Some(assistant_message),
                    thinking: _,
                    tool_invocations,
                    failure_message: Some(failure_message),
                }
            }
                if turn_id.starts_with("turn-")
                && started_at_ms <= finished_at_ms
                && !source_entry_ids.is_empty()
                && user_message == "你好"
                && assistant_message == "已收到：你好"
                && failure_message.contains("工具结果不匹配")
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
}
