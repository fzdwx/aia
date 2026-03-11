use std::{fmt, fs, path::Path};

use agent_core::{Message, Role, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

pub fn default_session_path() -> std::path::PathBuf {
    std::path::PathBuf::from(".like/session.jsonl")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionProviderBinding {
    Bootstrap,
    Provider { name: String, model: String, base_url: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub provider_binding: SessionProviderBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocationRecord {
    pub call: ToolCall,
    pub outcome: ToolInvocationRecordOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ToolInvocationRecordOutcome {
    Succeeded { result: ToolResult },
    Failed { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub turn_id: String,
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
    pub source_entry_ids: Vec<u64>,
    pub user_message: String,
    pub assistant_message: Option<String>,
    pub tool_invocations: Vec<ToolInvocationRecord>,
    pub failure_message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionFact {
    Message(Message),
    SessionMetadata(SessionMetadata),
    Anchor(AnchorState),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Turn(TurnRecord),
    Event(SessionEvent),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TapeEntry {
    pub id: u64,
    pub fact: SessionFact,
}

impl TapeEntry {
    pub fn message(&self) -> Option<&Message> {
        match &self.fact {
            SessionFact::Message(message) => Some(message),
            SessionFact::SessionMetadata(_)
            | SessionFact::Anchor(_)
            | SessionFact::ToolCall(_)
            | SessionFact::ToolResult(_)
            | SessionFact::Turn(_)
            | SessionFact::Event(_) => None,
        }
    }

    pub fn session_metadata(&self) -> Option<&SessionMetadata> {
        match &self.fact {
            SessionFact::SessionMetadata(metadata) => Some(metadata),
            SessionFact::Message(_)
            | SessionFact::Anchor(_)
            | SessionFact::ToolCall(_)
            | SessionFact::ToolResult(_)
            | SessionFact::Turn(_)
            | SessionFact::Event(_) => None,
        }
    }

    pub fn anchor_state(&self) -> Option<&AnchorState> {
        match &self.fact {
            SessionFact::Anchor(state) => Some(state),
            SessionFact::Message(_)
            | SessionFact::SessionMetadata(_)
            | SessionFact::ToolCall(_)
            | SessionFact::ToolResult(_)
            | SessionFact::Turn(_)
            | SessionFact::Event(_) => None,
        }
    }

    pub fn tool_call(&self) -> Option<&ToolCall> {
        match &self.fact {
            SessionFact::ToolCall(call) => Some(call),
            SessionFact::Message(_)
            | SessionFact::SessionMetadata(_)
            | SessionFact::Anchor(_)
            | SessionFact::ToolResult(_)
            | SessionFact::Turn(_)
            | SessionFact::Event(_) => None,
        }
    }

    pub fn tool_result(&self) -> Option<&ToolResult> {
        match &self.fact {
            SessionFact::ToolResult(result) => Some(result),
            SessionFact::Message(_)
            | SessionFact::SessionMetadata(_)
            | SessionFact::Anchor(_)
            | SessionFact::ToolCall(_)
            | SessionFact::Turn(_)
            | SessionFact::Event(_) => None,
        }
    }

    pub fn turn(&self) -> Option<&TurnRecord> {
        match &self.fact {
            SessionFact::Turn(turn) => Some(turn),
            SessionFact::Message(_)
            | SessionFact::SessionMetadata(_)
            | SessionFact::Anchor(_)
            | SessionFact::ToolCall(_)
            | SessionFact::ToolResult(_)
            | SessionFact::Event(_) => None,
        }
    }

    pub fn event(&self) -> Option<&SessionEvent> {
        match &self.fact {
            SessionFact::Event(event) => Some(event),
            SessionFact::Message(_)
            | SessionFact::SessionMetadata(_)
            | SessionFact::Anchor(_)
            | SessionFact::ToolCall(_)
            | SessionFact::ToolResult(_)
            | SessionFact::Turn(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub kind: String,
    pub detail: String,
    pub source_entry_ids: Vec<u64>,
}

impl SessionEvent {
    pub fn new(
        kind: impl Into<String>,
        detail: impl Into<String>,
        source_entry_ids: Vec<u64>,
    ) -> Self {
        Self { kind: kind.into(), detail: detail.into(), source_entry_ids }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnchorState {
    pub phase: String,
    pub summary: String,
    pub next_steps: Vec<String>,
    pub source_entry_ids: Vec<u64>,
    pub owner: String,
}

impl AnchorState {
    pub fn new(
        phase: impl Into<String>,
        summary: impl Into<String>,
        next_steps: Vec<String>,
        source_entry_ids: Vec<u64>,
        owner: impl Into<String>,
    ) -> Self {
        Self {
            phase: phase.into(),
            summary: summary.into(),
            next_steps,
            source_entry_ids,
            owner: owner.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub entry_id: u64,
    pub state: AnchorState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Handoff {
    pub anchor: Anchor,
    pub event_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    pub origin_anchor: Option<Anchor>,
    pub entries: Vec<TapeEntry>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTape {
    next_id: u64,
    entries: Vec<TapeEntry>,
}

impl Default for SessionTape {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTape {
    pub fn new() -> Self {
        Self { next_id: 1, entries: Vec::new() }
    }

    pub fn append(&mut self, message: Message) -> u64 {
        self.append_fact(SessionFact::Message(message))
    }

    pub fn append_fact(&mut self, fact: SessionFact) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(TapeEntry { id, fact });
        id
    }

    pub fn anchor(&mut self, state: AnchorState) -> Anchor {
        Anchor { entry_id: self.append_fact(SessionFact::Anchor(state.clone())), state }
    }

    pub fn bind_provider(&mut self, provider_binding: SessionProviderBinding) -> u64 {
        self.append_fact(SessionFact::SessionMetadata(SessionMetadata { provider_binding }))
    }

    pub fn handoff(&mut self, summary: impl Into<String>, next_steps: Vec<String>) -> Handoff {
        let summary = summary.into();
        let source_entry_ids =
            self.default_view().entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
        let state =
            AnchorState::new("handoff", summary.clone(), next_steps, source_entry_ids, "agent");
        let anchor = self.anchor(state);
        let event_id = self.append_fact(SessionFact::Event(SessionEvent::new(
            "handoff",
            summary,
            vec![anchor.entry_id],
        )));
        Handoff { anchor, event_id }
    }

    pub fn latest_anchor(&self) -> Option<Anchor> {
        self.entries.iter().rev().find_map(Self::anchor_from_entry)
    }

    pub fn assemble_view(&self, anchor: Option<&Anchor>) -> SessionView {
        let lower_bound = anchor.map(|value| value.entry_id).unwrap_or(0);
        let entries =
            self.entries.iter().filter(|entry| entry.id > lower_bound).cloned().collect::<Vec<_>>();
        let messages = entries.iter().filter_map(project_message).collect::<Vec<_>>();

        SessionView { origin_anchor: anchor.cloned(), entries, messages }
    }

    pub fn view_from(&self, anchor: Option<&Anchor>) -> Vec<Message> {
        self.assemble_view(anchor).messages
    }

    pub fn default_view(&self) -> SessionView {
        let latest_anchor = self.latest_anchor();
        self.assemble_view(latest_anchor.as_ref())
    }

    pub fn default_messages(&self) -> Vec<Message> {
        self.default_view().messages
    }

    pub fn latest_provider_binding(&self) -> Option<SessionProviderBinding> {
        self.entries
            .iter()
            .rev()
            .find_map(|entry| entry.session_metadata().map(|meta| meta.provider_binding.clone()))
    }

    pub fn replay_turns(&self) -> Vec<TurnRecord> {
        self.entries.iter().filter_map(|entry| entry.turn().cloned()).collect()
    }

    pub fn load_jsonl_or_default(path: &Path) -> Result<Self, SessionTapeError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let contents =
            fs::read_to_string(path).map_err(|error| SessionTapeError::new(error.to_string()))?;
        let mut entries = Vec::new();
        let mut next_id = 1;

        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let entry: TapeEntry = serde_json::from_str(line)
                .map_err(|error| SessionTapeError::new(error.to_string()))?;
            next_id = next_id.max(entry.id + 1);
            entries.push(entry);
        }

        Ok(Self { next_id, entries })
    }

    pub fn save_jsonl(&self, path: &Path) -> Result<(), SessionTapeError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| SessionTapeError::new(error.to_string()))?;
        }

        let contents = self
            .entries
            .iter()
            .map(|entry| {
                serde_json::to_string(entry)
                    .map_err(|error| SessionTapeError::new(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        let contents = if contents.is_empty() { contents } else { format!("{contents}\n") };
        fs::write(path, contents).map_err(|error| SessionTapeError::new(error.to_string()))
    }

    pub fn entries(&self) -> &[TapeEntry] {
        &self.entries
    }

    pub fn anchors(&self) -> Vec<Anchor> {
        self.entries.iter().filter_map(Self::anchor_from_entry).collect()
    }

    fn anchor_from_entry(entry: &TapeEntry) -> Option<Anchor> {
        entry.anchor_state().cloned().map(|state| Anchor { entry_id: entry.id, state })
    }
}

fn project_message(entry: &TapeEntry) -> Option<Message> {
    if let Some(message) = entry.message() {
        return Some(message.clone());
    }

    entry.tool_result().map(|result| {
        Message::new(
            Role::Tool,
            format!("工具 {} #{} 输出: {}", result.tool_name, result.invocation_id, result.content),
        )
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionTapeError {
    message: String,
}

impl SessionTapeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for SessionTapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SessionTapeError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use agent_core::{Message, Role, ToolCall, ToolResult};

    use super::{
        AnchorState, SessionFact, SessionProviderBinding, SessionTape, ToolInvocationRecord,
        ToolInvocationRecordOutcome, TurnRecord,
    };

    fn temp_file(name: &str) -> PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
        std::env::temp_dir().join(format!("like-session-{name}-{suffix}.jsonl"))
    }

    #[test]
    fn 默认会话路径位于项目隐藏目录() {
        assert_eq!(super::default_session_path(), PathBuf::from(".like/session.jsonl"));
    }

    #[test]
    fn 会记住最近一次_provider_绑定() {
        let mut tape = SessionTape::new();
        tape.bind_provider(SessionProviderBinding::Bootstrap);
        tape.bind_provider(SessionProviderBinding::Provider {
            name: "main".into(),
            model: "gpt-4.1-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
        });

        assert_eq!(
            tape.latest_provider_binding(),
            Some(SessionProviderBinding::Provider {
                name: "main".into(),
                model: "gpt-4.1-mini".into(),
                base_url: "https://api.openai.com/v1".into(),
            })
        );
    }

    #[test]
    fn 锚点以追加事实形式保留在磁带中() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "第一轮"));
        let anchor = tape.anchor(AnchorState::new(
            "discovery",
            "发现完成",
            vec!["进入实现".into()],
            vec![1],
            "agent",
        ));
        tape.append(Message::new(Role::Assistant, "第二轮"));

        assert_eq!(anchor.entry_id, 2);
        assert_eq!(tape.entries().len(), 3);
        assert!(matches!(tape.entries()[1].fact, SessionFact::Anchor(_)));
        assert_eq!(tape.anchors().len(), 1);
    }

    #[test]
    fn 锚点之后可以按视图重建消息() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "第一轮"));
        tape.append(Message::new(Role::Assistant, "第一轮回复"));
        let anchor = tape.anchor(AnchorState::new(
            "implement",
            "实现开始",
            vec!["写代码".into()],
            vec![1, 2],
            "agent",
        ));
        tape.append(Message::new(Role::User, "第二轮"));

        let view = tape.assemble_view(Some(&anchor));

        assert_eq!(view.origin_anchor.as_ref().map(|value| value.entry_id), Some(anchor.entry_id));
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].content, "第二轮");
    }

    #[test]
    fn 交接会把当前阶段来源写入结构化锚点() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "开始"));
        tape.append(Message::new(Role::Assistant, "完成发现"));

        let handoff = tape.handoff("移交给实现阶段", vec!["实现运行时".into()]);

        assert_eq!(handoff.anchor.state.phase, "handoff");
        assert_eq!(handoff.anchor.state.summary, "移交给实现阶段");
        assert_eq!(handoff.anchor.state.next_steps, vec!["实现运行时"]);
        assert_eq!(handoff.anchor.state.source_entry_ids, vec![1, 2]);
        assert_eq!(handoff.anchor.state.owner, "agent");
        assert_eq!(handoff.event_id, 4);
        assert!(matches!(tape.entries()[3].fact, SessionFact::Event(_)));
    }

    #[test]
    fn 最新锚点可从事实磁带中推导() {
        let mut tape = SessionTape::new();
        let first = tape.anchor(AnchorState::new("d1", "第一阶段", vec![], vec![], "agent"));
        let second = tape.anchor(AnchorState::new("d2", "第二阶段", vec![], vec![], "agent"));

        assert_eq!(tape.latest_anchor(), Some(second));
        assert_ne!(tape.latest_anchor(), Some(first));
    }

    #[test]
    fn 默认视图从最新锚点之后组装() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "第一轮"));
        let _ = tape.anchor(AnchorState::new("d1", "阶段一", vec![], vec![1], "agent"));
        tape.append(Message::new(Role::Assistant, "第二轮"));

        let messages = tape.default_messages();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "第二轮");
    }

    #[test]
    fn 工具调用与结果可作为类型化事实保留() {
        let mut tape = SessionTape::new();
        let call = ToolCall::new("search_code").with_argument("query", "session-tape");
        let call_id = tape.append_fact(SessionFact::ToolCall(call.clone()));
        let result_id = tape
            .append_fact(SessionFact::ToolResult(ToolResult::from_call(&call, "found 3 matches")));

        assert_eq!(call_id, 1);
        assert_eq!(result_id, 2);
        assert_eq!(
            tape.entries()[0].tool_call().map(|value| value.tool_name.as_str()),
            Some("search_code")
        );
        assert_eq!(
            tape.entries()[1].tool_result().map(|value| value.content.as_str()),
            Some("found 3 matches")
        );
        assert_eq!(
            tape.entries()[0].tool_call().expect("应有工具调用").invocation_id,
            tape.entries()[1].tool_result().expect("应有工具结果").invocation_id,
        );
    }

    #[test]
    fn 工具结果投影到默认视图时保留调用标识() {
        let mut tape = SessionTape::new();
        let call = ToolCall::new("search_code").with_argument("query", "session-tape");
        let _ = tape.append_fact(SessionFact::ToolResult(ToolResult::from_call(&call, "ok")));

        let messages = tape.default_messages();

        assert_eq!(messages.len(), 1);
        assert!(messages[0].content.contains("工具 search_code #"));
        assert!(messages[0].content.contains(&call.invocation_id));
    }

    #[test]
    fn 轮次块可作为可重放索引保存并重新载入() {
        let path = temp_file("turn-replay");
        let mut tape = SessionTape::new();
        let call = ToolCall::new("search_code").with_argument("query", "runtime");
        let turn = TurnRecord {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1, 2, 3],
            user_message: "你好".into(),
            assistant_message: Some("已收到：你好".into()),
            tool_invocations: vec![ToolInvocationRecord {
                call: call.clone(),
                outcome: ToolInvocationRecordOutcome::Succeeded {
                    result: ToolResult::from_call(&call, "未实现"),
                },
            }],
            failure_message: None,
        };
        tape.append_fact(SessionFact::Turn(turn.clone()));

        tape.save_jsonl(&path).expect("保存成功");
        let restored = SessionTape::load_jsonl_or_default(&path).expect("载入成功");

        assert_eq!(restored.replay_turns(), vec![turn]);
        let _ = fs::remove_file(path);
    }
}
