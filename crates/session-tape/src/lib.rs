use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{Message, Role, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn default_session_path() -> std::path::PathBuf {
    std::path::PathBuf::from(".aia/session.jsonl")
}

// ---------------------------------------------------------------------------
// SessionProviderBinding — kept as-is
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionProviderBinding {
    Bootstrap,
    Provider { name: String, model: String, base_url: String },
}

// ---------------------------------------------------------------------------
// TapeEntry — flat {id, kind, payload, meta, date}
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TapeEntry {
    pub id: u64,
    pub kind: String,
    pub payload: Value,
    #[serde(default = "default_meta")]
    pub meta: Value,
    #[serde(default)]
    pub date: String,
}

fn default_meta() -> Value {
    Value::Object(serde_json::Map::new())
}

// -- Factory methods (id=0 placeholder, assigned by SessionTape::append_entry) --

impl TapeEntry {
    fn new(kind: &str, payload: Value) -> Self {
        Self { id: 0, kind: kind.to_string(), payload, meta: default_meta(), date: now_iso8601() }
    }

    pub fn message(msg: &Message) -> Self {
        Self::new("message", serde_json::to_value(msg).unwrap_or_default())
    }

    pub fn system(content: &str) -> Self {
        Self::new("system", serde_json::json!({"content": content}))
    }

    pub fn anchor(name: &str, state: Option<Value>) -> Self {
        Self::new(
            "anchor",
            serde_json::json!({
                "name": name,
                "state": state.unwrap_or(Value::Object(serde_json::Map::new()))
            }),
        )
    }

    pub fn tool_call(call: &ToolCall) -> Self {
        Self::new("tool_call", serde_json::to_value(call).unwrap_or_default())
    }

    pub fn tool_result(result: &ToolResult) -> Self {
        Self::new("tool_result", serde_json::to_value(result).unwrap_or_default())
    }

    pub fn event(name: &str, data: Option<Value>) -> Self {
        Self::new(
            "event",
            serde_json::json!({
                "name": name,
                "data": data.unwrap_or(Value::Null)
            }),
        )
    }

    pub fn error(message: &str) -> Self {
        Self::new("error", serde_json::json!({"message": message}))
    }

    // -- Builder --

    pub fn with_meta(mut self, key: &str, value: Value) -> Self {
        if let Value::Object(ref mut map) = self.meta {
            map.insert(key.to_string(), value);
        }
        self
    }

    pub fn with_run_id(self, run_id: &str) -> Self {
        self.with_meta("run_id", Value::String(run_id.to_string()))
    }

    // -- Typed accessors --

    pub fn as_message(&self) -> Option<Message> {
        if self.kind == "message" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    pub fn as_tool_call(&self) -> Option<ToolCall> {
        if self.kind == "tool_call" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    pub fn as_tool_result(&self) -> Option<ToolResult> {
        if self.kind == "tool_result" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    pub fn anchor_name(&self) -> Option<&str> {
        if self.kind == "anchor" { self.payload.get("name").and_then(|v| v.as_str()) } else { None }
    }

    pub fn anchor_state(&self) -> Option<&Value> {
        if self.kind == "anchor" { self.payload.get("state") } else { None }
    }

    pub fn event_name(&self) -> Option<&str> {
        if self.kind == "event" { self.payload.get("name").and_then(|v| v.as_str()) } else { None }
    }

    pub fn event_data(&self) -> Option<&Value> {
        if self.kind == "event" { self.payload.get("data") } else { None }
    }

    pub fn thinking(content: &str) -> Self {
        TapeEntry::new("thinking", serde_json::json!({"content": content}))
    }

    pub fn as_thinking(&self) -> Option<&str> {
        if self.kind == "thinking" {
            self.payload.get("content").and_then(|v| v.as_str())
        } else {
            None
        }
    }

    fn matches_text(&self, pattern: &str) -> bool {
        let lowered = pattern.to_lowercase();
        let haystack = self.payload.to_string().to_lowercase();
        if haystack.contains(&lowered) {
            return true;
        }
        self.kind.to_lowercase().contains(&lowered)
    }
}

// ---------------------------------------------------------------------------
// Anchor / Handoff / SessionView — simplified
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub entry_id: u64,
    pub name: String,
    pub state: Value,
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

// ---------------------------------------------------------------------------
// SessionTape
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTape {
    name: String,
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
        Self::named("session")
    }

    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), next_id: 1, entries: Vec::new() }
    }

    pub fn append(&mut self, message: Message) -> u64 {
        self.append_entry(TapeEntry::message(&message))
    }

    pub fn append_entry(&mut self, mut entry: TapeEntry) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        entry.id = id;
        self.entries.push(entry);
        id
    }

    /// Load a persisted entry (id already assigned). Validates sequential ordering.
    fn load_persisted_entry(&mut self, entry: TapeEntry) -> Result<u64, SessionTapeError> {
        if entry.id != self.next_id {
            return Err(SessionTapeError::new(format!(
                "磁带 {} 追加记录 id 不连续：期待 {}，收到 {}",
                self.name, self.next_id, entry.id
            )));
        }
        let id = entry.id;
        self.next_id += 1;
        self.entries.push(entry);
        Ok(id)
    }

    pub fn anchor(&mut self, name: impl Into<String>, state: Option<Value>) -> Anchor {
        let name = name.into();
        let state_val = state.unwrap_or(Value::Object(serde_json::Map::new()));
        let id = self.append_entry(TapeEntry::anchor(&name, Some(state_val.clone())));
        Anchor { entry_id: id, name, state: state_val }
    }

    pub fn bind_provider(&mut self, provider_binding: SessionProviderBinding) -> u64 {
        self.append_entry(TapeEntry::event(
            "provider_binding",
            Some(serde_json::to_value(&provider_binding).unwrap_or_default()),
        ))
    }

    pub fn latest_provider_binding(&self) -> Option<SessionProviderBinding> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| entry.kind == "event")
            .filter(|entry| entry.event_name() == Some("provider_binding"))
            .find_map(|entry| {
                entry.event_data().and_then(|data| serde_json::from_value(data.clone()).ok())
            })
    }

    pub fn handoff(&mut self, name: impl Into<String>, state: Value) -> Handoff {
        let name = name.into();
        let anchor = self.anchor(&name, Some(state));
        let event_id = self.append_entry(TapeEntry::event(
            "handoff",
            Some(serde_json::json!({"anchor_entry_id": anchor.entry_id})),
        ));
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

    pub fn tape_name(&self) -> &str {
        &self.name
    }

    pub fn entries(&self) -> &[TapeEntry] {
        &self.entries
    }

    pub fn anchors(&self) -> Vec<Anchor> {
        self.entries.iter().filter_map(Self::anchor_from_entry).collect()
    }

    pub fn query_entries(&self, query: TapeQuery) -> Result<Vec<TapeEntry>, SessionTapeError> {
        let lower_bound = self.query_lower_bound(&query)?;
        let upper_bound = self.query_upper_bound(&query, lower_bound)?;
        let mut results: Vec<&TapeEntry> = self
            .entries
            .iter()
            .filter(|entry| entry.id > lower_bound && upper_bound.is_none_or(|ub| entry.id < ub))
            .collect();
        if let Some(start) = query.start_date.as_ref() {
            results.retain(|entry| !entry.date.is_empty() && entry.date.as_str() >= start.as_str());
        }
        if let Some(end) = query.end_date.as_ref() {
            results.retain(|entry| !entry.date.is_empty() && entry.date.as_str() <= end.as_str());
        }
        if !query.kinds.is_empty() {
            results.retain(|entry| query.kinds.contains(&entry.kind));
        }
        if let Some(pattern) = query.text.as_ref() {
            results.retain(|entry| entry.matches_text(pattern));
        }
        let mut out: Vec<TapeEntry> = results.into_iter().cloned().collect();
        if let Some(limit) = query.limit {
            out.truncate(limit);
        }
        Ok(out)
    }

    pub fn fork(&self, fork_name: impl Into<String>) -> SessionTapeFork {
        SessionTapeFork {
            parent_name: self.name.clone(),
            base_len: self.entries.len(),
            base_next_id: self.next_id,
            tape: Self {
                name: fork_name.into(),
                next_id: self.next_id,
                entries: self.entries.clone(),
            },
        }
    }

    pub fn load_jsonl_or_default(path: &Path) -> Result<Self, SessionTapeError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let contents = fs::read_to_string(path).map_err(SessionTapeError::from_io)?;
        let mut tape = Self::new();
        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let entry = decode_persisted_line(line)?;
            tape.load_persisted_entry(entry)?;
        }

        Ok(tape)
    }

    pub fn save_jsonl(&self, path: &Path) -> Result<(), SessionTapeError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SessionTapeError::from_io)?;
        }

        let contents = self
            .entries
            .iter()
            .map(|entry| serde_json::to_string(entry).map_err(SessionTapeError::from_serde))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        let contents = if contents.is_empty() { contents } else { format!("{contents}\n") };
        fs::write(path, contents).map_err(SessionTapeError::from_io)
    }

    fn anchor_from_entry(entry: &TapeEntry) -> Option<Anchor> {
        if entry.kind != "anchor" {
            return None;
        }
        let name = entry.anchor_name()?.to_string();
        let state = entry.anchor_state().cloned().unwrap_or(Value::Object(serde_json::Map::new()));
        Some(Anchor { entry_id: entry.id, name, state })
    }

    fn query_lower_bound(&self, query: &TapeQuery) -> Result<u64, SessionTapeError> {
        if query.after_latest_anchor {
            return Ok(self.latest_anchor().map(|anchor| anchor.entry_id).unwrap_or(0));
        }
        if let Some(anchor_name) = query.after_anchor_name.as_ref() {
            return self
                .find_anchor(anchor_name)
                .map(|anchor| anchor.entry_id)
                .ok_or_else(|| SessionTapeError::new(format!("未找到锚点：{anchor_name}")));
        }
        Ok(query.after_entry_id.unwrap_or(0))
    }

    fn query_upper_bound(
        &self,
        query: &TapeQuery,
        lower_bound: u64,
    ) -> Result<Option<u64>, SessionTapeError> {
        let Some(anchor_name) = query.before_anchor_name.as_ref() else {
            return Ok(None);
        };
        let anchor = self
            .entries
            .iter()
            .filter(|entry| entry.id > lower_bound)
            .filter_map(Self::anchor_from_entry)
            .find(|anchor| anchor.name == *anchor_name)
            .ok_or_else(|| SessionTapeError::new(format!("未找到锚点：{anchor_name}")))?;
        Ok(Some(anchor.entry_id))
    }

    fn find_anchor(&self, anchor_name: &str) -> Option<Anchor> {
        self.entries
            .iter()
            .rev()
            .filter_map(Self::anchor_from_entry)
            .find(|anchor| anchor.name == anchor_name)
    }
}

// ---------------------------------------------------------------------------
// TapeQuery
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TapeQuery {
    after_entry_id: Option<u64>,
    after_latest_anchor: bool,
    after_anchor_name: Option<String>,
    before_anchor_name: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    kinds: BTreeSet<String>,
    text: Option<String>,
    limit: Option<usize>,
}

impl Default for TapeQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl TapeQuery {
    pub fn new() -> Self {
        Self {
            after_entry_id: None,
            after_latest_anchor: false,
            after_anchor_name: None,
            before_anchor_name: None,
            start_date: None,
            end_date: None,
            kinds: BTreeSet::new(),
            text: None,
            limit: None,
        }
    }

    pub fn after_latest_anchor(mut self) -> Self {
        self.after_latest_anchor = true;
        self
    }

    pub fn after_entry_id(mut self, entry_id: u64) -> Self {
        self.after_entry_id = Some(entry_id);
        self
    }

    pub fn after_anchor_name(mut self, anchor_name: impl Into<String>) -> Self {
        self.after_anchor_name = Some(anchor_name.into());
        self
    }

    pub fn before_anchor_name(mut self, anchor_name: impl Into<String>) -> Self {
        self.before_anchor_name = Some(anchor_name.into());
        self
    }

    pub fn between_anchor_names(
        mut self,
        after_anchor_name: impl Into<String>,
        before_anchor_name: impl Into<String>,
    ) -> Self {
        self.after_anchor_name = Some(after_anchor_name.into());
        self.before_anchor_name = Some(before_anchor_name.into());
        self
    }

    pub fn within_dates(mut self, start: impl Into<String>, end: impl Into<String>) -> Self {
        self.start_date = Some(start.into());
        self.end_date = Some(end.into());
        self
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kinds.insert(kind.into());
        self
    }

    pub fn matching_text(mut self, pattern: impl Into<String>) -> Self {
        self.text = Some(pattern.into());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

// ---------------------------------------------------------------------------
// NamedTapeStorage trait + impls
// ---------------------------------------------------------------------------

pub trait NamedTapeStorage {
    fn tape_names(&self) -> Vec<String>;
    fn load_tape(&self, tape_name: &str) -> Result<SessionTape, SessionTapeError>;
    fn save_tape(&mut self, tape: &SessionTape) -> Result<(), SessionTapeError>;
    fn append_entry_to(
        &mut self,
        tape_name: &str,
        entry: &TapeEntry,
    ) -> Result<(), SessionTapeError>;
}

#[derive(Default)]
pub struct InMemoryTapeStorage {
    tapes: BTreeMap<String, Vec<TapeEntry>>,
}

impl InMemoryTapeStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NamedTapeStorage for InMemoryTapeStorage {
    fn tape_names(&self) -> Vec<String> {
        self.tapes.keys().cloned().collect()
    }

    fn load_tape(&self, tape_name: &str) -> Result<SessionTape, SessionTapeError> {
        let Some(entries) = self.tapes.get(tape_name) else {
            return Ok(SessionTape::named(tape_name));
        };

        let mut tape = SessionTape::named(tape_name);
        for entry in entries.clone() {
            tape.load_persisted_entry(entry)?;
        }
        Ok(tape)
    }

    fn save_tape(&mut self, tape: &SessionTape) -> Result<(), SessionTapeError> {
        self.tapes.insert(tape.name.clone(), tape.entries.clone());
        Ok(())
    }

    fn append_entry_to(
        &mut self,
        tape_name: &str,
        entry: &TapeEntry,
    ) -> Result<(), SessionTapeError> {
        self.tapes.entry(tape_name.to_string()).or_default().push(entry.clone());
        Ok(())
    }
}

pub struct JsonlTapeStorage {
    root_dir: PathBuf,
}

impl JsonlTapeStorage {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self { root_dir: root_dir.into() }
    }

    fn tape_path(&self, tape_name: &str) -> PathBuf {
        self.root_dir.join(format!("{tape_name}.jsonl"))
    }
}

impl NamedTapeStorage for JsonlTapeStorage {
    fn tape_names(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.root_dir) else {
            return Vec::new();
        };

        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                    return None;
                }
                path.file_stem().and_then(|value| value.to_str()).map(str::to_string)
            })
            .collect()
    }

    fn load_tape(&self, tape_name: &str) -> Result<SessionTape, SessionTapeError> {
        let path = self.tape_path(tape_name);
        if !path.exists() {
            return Ok(SessionTape::named(tape_name));
        }

        let file = fs::File::open(&path).map_err(SessionTapeError::from_io)?;
        let reader = BufReader::new(file);
        let mut tape = SessionTape::named(tape_name);
        for line in reader.lines() {
            let line = line.map_err(SessionTapeError::from_io)?;
            if line.trim().is_empty() {
                continue;
            }
            let entry = decode_persisted_line(&line)?;
            tape.load_persisted_entry(entry)?;
        }
        Ok(tape)
    }

    fn save_tape(&mut self, tape: &SessionTape) -> Result<(), SessionTapeError> {
        let path = self.tape_path(tape.tape_name());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SessionTapeError::from_io)?;
        }
        let contents = tape
            .entries
            .iter()
            .map(|entry| serde_json::to_string(entry).map_err(SessionTapeError::from_serde))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        let contents = if contents.is_empty() { contents } else { format!("{contents}\n") };
        fs::write(path, contents).map_err(SessionTapeError::from_io)
    }

    fn append_entry_to(
        &mut self,
        tape_name: &str,
        entry: &TapeEntry,
    ) -> Result<(), SessionTapeError> {
        let path = self.tape_path(tape_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SessionTapeError::from_io)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(SessionTapeError::from_io)?;
        let line = serde_json::to_string(entry).map_err(SessionTapeError::from_serde)?;
        writeln!(file, "{line}").map_err(SessionTapeError::from_io)
    }
}

// ---------------------------------------------------------------------------
// SessionTapeFork
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTapeFork {
    parent_name: String,
    base_len: usize,
    base_next_id: u64,
    tape: SessionTape,
}

impl SessionTapeFork {
    pub fn append(&mut self, message: Message) -> u64 {
        self.tape.append(message)
    }

    pub fn append_entry(&mut self, entry: TapeEntry) -> u64 {
        self.tape.append_entry(entry)
    }

    pub fn handoff(&mut self, name: impl Into<String>, state: Value) -> Handoff {
        self.tape.handoff(name, state)
    }

    pub fn merge_into(self, parent: &mut SessionTape) -> Result<Vec<u64>, SessionTapeError> {
        if parent.tape_name() != self.parent_name {
            return Err(SessionTapeError::new(format!(
                "分叉父磁带不匹配：期待 {}，收到 {}",
                self.parent_name,
                parent.tape_name()
            )));
        }
        if parent.next_id != self.base_next_id || parent.entries.len() != self.base_len {
            return Err(SessionTapeError::new(format!(
                "父磁带 {} 在分叉后已变化，无法直接回放增量",
                parent.tape_name()
            )));
        }
        let delta = self.tape.entries[self.base_len..].to_vec();
        let merged_ids = delta.iter().map(|entry| entry.id).collect::<Vec<_>>();
        parent.entries.extend(delta);
        parent.next_id = self.tape.next_id;
        Ok(merged_ids)
    }
}

// ---------------------------------------------------------------------------
// Legacy compat — decode old {id, fact, date} format
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LegacyEntry {
    id: u64,
    fact: LegacyFact,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Serialize, Deserialize)]
enum LegacyFact {
    Message(Message),
    SessionMetadata(LegacySessionMetadata),
    Anchor(LegacyAnchorState),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Turn(Value),
    Event(LegacySessionEvent),
    Error(LegacyErrorRecord),
}

#[derive(Serialize, Deserialize)]
struct LegacySessionMetadata {
    provider_binding: SessionProviderBinding,
}

#[derive(Serialize, Deserialize)]
struct LegacyAnchorState {
    #[serde(default = "default_anchor_name")]
    name: String,
    phase: String,
    summary: String,
    next_steps: Vec<String>,
    source_entry_ids: Vec<u64>,
    owner: String,
}

fn default_anchor_name() -> String {
    "anchor".into()
}

#[derive(Serialize, Deserialize)]
struct LegacySessionEvent {
    kind: String,
    detail: String,
    source_entry_ids: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
struct LegacyErrorRecord {
    message: String,
    #[serde(default)]
    source_entry_ids: Vec<u64>,
}

fn convert_legacy(legacy: LegacyEntry) -> TapeEntry {
    let date = legacy.date.unwrap_or_default();
    match legacy.fact {
        LegacyFact::Message(msg) => TapeEntry {
            id: legacy.id,
            kind: "message".into(),
            payload: serde_json::to_value(&msg).unwrap_or_default(),
            meta: default_meta(),
            date,
        },
        LegacyFact::SessionMetadata(meta) => TapeEntry {
            id: legacy.id,
            kind: "event".into(),
            payload: serde_json::json!({
                "name": "provider_binding",
                "data": serde_json::to_value(&meta.provider_binding).unwrap_or_default()
            }),
            meta: default_meta(),
            date,
        },
        LegacyFact::Anchor(anchor) => TapeEntry {
            id: legacy.id,
            kind: "anchor".into(),
            payload: serde_json::json!({
                "name": anchor.name,
                "state": {
                    "phase": anchor.phase,
                    "summary": anchor.summary,
                    "next_steps": anchor.next_steps,
                    "source_entry_ids": anchor.source_entry_ids,
                    "owner": anchor.owner,
                }
            }),
            meta: default_meta(),
            date,
        },
        LegacyFact::ToolCall(call) => TapeEntry {
            id: legacy.id,
            kind: "tool_call".into(),
            payload: serde_json::to_value(&call).unwrap_or_default(),
            meta: default_meta(),
            date,
        },
        LegacyFact::ToolResult(result) => TapeEntry {
            id: legacy.id,
            kind: "tool_result".into(),
            payload: serde_json::to_value(&result).unwrap_or_default(),
            meta: default_meta(),
            date,
        },
        LegacyFact::Turn(value) => TapeEntry {
            id: legacy.id,
            kind: "event".into(),
            payload: serde_json::json!({"name": "turn_record", "data": value}),
            meta: default_meta(),
            date,
        },
        LegacyFact::Event(event) => TapeEntry {
            id: legacy.id,
            kind: "event".into(),
            payload: serde_json::json!({
                "name": event.kind,
                "data": {"detail": event.detail}
            }),
            meta: serde_json::json!({"source_entry_ids": event.source_entry_ids}),
            date,
        },
        LegacyFact::Error(error) => TapeEntry {
            id: legacy.id,
            kind: "error".into(),
            payload: serde_json::json!({"message": error.message}),
            meta: serde_json::json!({"source_entry_ids": error.source_entry_ids}),
            date,
        },
    }
}

fn decode_persisted_line(line: &str) -> Result<TapeEntry, SessionTapeError> {
    // Try new flat format first
    if let Ok(entry) = serde_json::from_str::<TapeEntry>(line) {
        if !entry.kind.is_empty() {
            return Ok(entry);
        }
    }
    // Try legacy {id, fact, date} format
    if let Ok(legacy) = serde_json::from_str::<LegacyEntry>(line) {
        return Ok(convert_legacy(legacy));
    }
    Err(SessionTapeError::from_serde(serde_json::from_str::<Value>(line).unwrap_err()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn project_message(entry: &TapeEntry) -> Option<Message> {
    // Thinking entries are display-only — never injected into conversation context.
    if entry.kind == "thinking" {
        return None;
    }

    if let Some(message) = entry.as_message() {
        return Some(message);
    }

    entry.as_tool_result().map(|result| {
        Message::new(
            Role::Tool,
            format!("工具 {} #{} 输出: {}", result.tool_name, result.invocation_id, result.content),
        )
    })
}

fn now_iso8601() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let day_secs = (secs % 86400) as u32;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;
    let sec = day_secs % 60;

    // civil_from_days — Howard Hinnant algorithm
    let z = (secs / 86400) as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{mins:02}:{sec:02}Z")
}

// ---------------------------------------------------------------------------
// SessionTapeError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionTapeError {
    message: String,
}

impl SessionTapeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    fn from_io(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }

    fn from_serde(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl fmt::Display for SessionTapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SessionTapeError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use agent_core::{Message, Role, ToolCall, ToolResult};
    use serde_json::json;

    use super::{SessionProviderBinding, SessionTape, TapeEntry, TapeQuery};

    fn temp_file(name: &str) -> PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
        std::env::temp_dir().join(format!("aia-session-{name}-{suffix}.jsonl"))
    }

    #[test]
    fn 默认会话路径位于项目隐藏目录() {
        assert_eq!(super::default_session_path(), PathBuf::from(".aia/session.jsonl"));
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
    fn 锚点以追加条目形式保留在磁带中() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "第一轮"));
        let anchor = tape
            .anchor("discovery", Some(json!({"summary": "发现完成", "next_steps": ["进入实现"]})));
        tape.append(Message::new(Role::Assistant, "第二轮"));

        assert_eq!(anchor.entry_id, 2);
        assert_eq!(tape.entries().len(), 3);
        assert_eq!(tape.entries()[1].kind, "anchor");
        assert_eq!(tape.anchors().len(), 1);
    }

    #[test]
    fn 锚点之后可以按视图重建消息() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "第一轮"));
        tape.append(Message::new(Role::Assistant, "第一轮回复"));
        let anchor = tape
            .anchor("implement", Some(json!({"summary": "实现开始", "next_steps": ["写代码"]})));
        tape.append(Message::new(Role::User, "第二轮"));

        let view = tape.assemble_view(Some(&anchor));

        assert_eq!(view.origin_anchor.as_ref().map(|value| value.entry_id), Some(anchor.entry_id));
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].content, "第二轮");
    }

    #[test]
    fn 交接会创建锚点和事件() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "开始"));
        tape.append(Message::new(Role::Assistant, "完成发现"));

        let handoff = tape
            .handoff("handoff", json!({"summary": "移交给实现阶段", "next_steps": ["实现运行时"]}));

        assert_eq!(
            handoff.anchor.state.get("summary").and_then(|v| v.as_str()),
            Some("移交给实现阶段")
        );
        assert!(handoff.event_id > handoff.anchor.entry_id);
        assert_eq!(tape.entries()[2].kind, "anchor");
        assert_eq!(tape.entries()[3].kind, "event");
    }

    #[test]
    fn 最新锚点可从磁带中推导() {
        let mut tape = SessionTape::new();
        let first = tape.anchor("d1", Some(json!({"summary": "第一阶段"})));
        let second = tape.anchor("d2", Some(json!({"summary": "第二阶段"})));

        assert_eq!(tape.latest_anchor(), Some(second));
        assert_ne!(tape.latest_anchor(), Some(first));
    }

    #[test]
    fn 默认视图从最新锚点之后组装() {
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "第一轮"));
        let _ = tape.anchor("d1", Some(json!({"summary": "阶段一"})));
        tape.append(Message::new(Role::Assistant, "第二轮"));

        let messages = tape.default_messages();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "第二轮");
    }

    #[test]
    fn 工具调用与结果可作为条目保留() {
        let mut tape = SessionTape::new();
        let call = ToolCall::new("search_code").with_argument("query", "session-tape");
        let call_id = tape.append_entry(TapeEntry::tool_call(&call));
        let result_id = tape
            .append_entry(TapeEntry::tool_result(&ToolResult::from_call(&call, "found 3 matches")));

        assert_eq!(call_id, 1);
        assert_eq!(result_id, 2);
        assert_eq!(
            tape.entries()[0].as_tool_call().map(|value| value.tool_name.clone()),
            Some("search_code".into())
        );
        assert_eq!(
            tape.entries()[1].as_tool_result().map(|value| value.content.clone()),
            Some("found 3 matches".into())
        );
        assert_eq!(
            tape.entries()[0].as_tool_call().expect("应有工具调用").invocation_id,
            tape.entries()[1].as_tool_result().expect("应有工具结果").invocation_id,
        );
    }

    #[test]
    fn 工具结果投影到默认视图时保留调用标识() {
        let mut tape = SessionTape::new();
        let call = ToolCall::new("search_code").with_argument("query", "session-tape");
        let _ = tape.append_entry(TapeEntry::tool_result(&ToolResult::from_call(&call, "ok")));

        let messages = tape.default_messages();

        assert_eq!(messages.len(), 1);
        assert!(messages[0].content.contains("工具 search_code #"));
        assert!(messages[0].content.contains(&call.invocation_id));
    }

    #[test]
    fn 保存并载入新格式_jsonl() {
        let path = temp_file("new-format");
        let mut tape = SessionTape::new();
        tape.append(Message::new(Role::User, "你好"));
        tape.append_entry(
            TapeEntry::event("turn_started", Some(json!({"turn_id": "turn-1"})))
                .with_run_id("turn-1"),
        );

        tape.save_jsonl(&path).expect("保存成功");
        let restored = SessionTape::load_jsonl_or_default(&path).expect("载入成功");

        assert_eq!(restored.entries().len(), 2);
        assert_eq!(
            restored.entries()[0].as_message().map(|m| m.content.clone()),
            Some("你好".into())
        );
        assert_eq!(restored.entries()[1].event_name(), Some("turn_started"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn 兼容载入旧格式_jsonl() {
        let path = temp_file("legacy-compat");
        // Write old format: {id, fact: Message(...), date: "..."}
        let old_line = r#"{"id":1,"fact":{"Message":{"role":"User","content":"旧文件兼容"}},"date":"2026-03-12T10:00:00Z"}"#;
        fs::write(&path, format!("{old_line}\n")).expect("写入成功");

        let mut tape = SessionTape::load_jsonl_or_default(&path).expect("载入成功");
        tape.append(Message::new(Role::Assistant, "继续运行"));
        tape.save_jsonl(&path).expect("保存成功");

        // New format should use kind/payload
        let contents = fs::read_to_string(&path).expect("读取成功");
        assert!(contents.lines().all(|line| line.contains("\"kind\"")));

        let restored = SessionTape::load_jsonl_or_default(&path).expect("再次载入成功");
        assert_eq!(restored.entries().len(), 2);
        assert_eq!(
            restored.entries()[0].as_message().map(|value| value.content.clone()),
            Some("旧文件兼容".into())
        );
        assert_eq!(restored.entries()[0].date, "2026-03-12T10:00:00Z");
        assert_eq!(
            restored.entries()[1].as_message().map(|value| value.content.clone()),
            Some("继续运行".into())
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn 查询可按锚点区间日期类型文本与数量切片() {
        let mut tape = SessionTape::named("session");
        let e1 = TapeEntry {
            id: 0,
            kind: "message".into(),
            payload: serde_json::to_value(&Message::new(Role::User, "alpha start")).unwrap(),
            meta: super::default_meta(),
            date: "2026-03-10T00:00:00Z".into(),
        };
        tape.append_entry(e1);

        let _ = tape.anchor("phase-a", Some(json!({"summary": "阶段 A"})));
        // Manually fix the date on the anchor entry
        tape.entries.last_mut().unwrap().date = "2026-03-11T00:00:00Z".into();

        let e3 = TapeEntry {
            id: 0,
            kind: "message".into(),
            payload: serde_json::to_value(&Message::new(
                Role::Assistant,
                "alpha implementation note",
            ))
            .unwrap(),
            meta: super::default_meta(),
            date: "2026-03-12T00:00:00Z".into(),
        };
        tape.append_entry(e3);

        let e4 = TapeEntry {
            id: 0,
            kind: "tool_result".into(),
            payload: serde_json::to_value(&ToolResult {
                invocation_id: "tool-call-1".into(),
                tool_name: "search".into(),
                content: "alpha result".into(),
            })
            .unwrap(),
            meta: super::default_meta(),
            date: "2026-03-13T00:00:00Z".into(),
        };
        tape.append_entry(e4);

        let _ = tape.anchor("phase-b", Some(json!({"summary": "阶段 B"})));
        tape.entries.last_mut().unwrap().date = "2026-03-14T00:00:00Z".into();

        let e6 = TapeEntry {
            id: 0,
            kind: "message".into(),
            payload: serde_json::to_value(&Message::new(Role::User, "omega finish")).unwrap(),
            meta: super::default_meta(),
            date: "2026-03-15T00:00:00Z".into(),
        };
        tape.append_entry(e6);

        let entries = tape
            .query_entries(
                TapeQuery::new()
                    .between_anchor_names("phase-a", "phase-b")
                    .with_kind("message")
                    .matching_text("alpha")
                    .within_dates("2026-03-12T00:00:00Z", "2026-03-13T23:59:59Z")
                    .limit(1),
            )
            .expect("查询成功");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 3);
        assert_eq!(
            entries[0].as_message().map(|value| value.content.clone()),
            Some("alpha implementation note".into())
        );
    }

    #[test]
    fn 分叉合并只回写增量条目() {
        let mut tape = SessionTape::named("session");
        tape.append(Message::new(Role::User, "base"));
        let mut fork = tape.fork("session-branch");
        fork.append(Message::new(Role::Assistant, "branch reply"));
        let handoff =
            fork.handoff("handoff", json!({"summary": "分叉交接", "next_steps": ["合并回主线"]}));

        let merged_ids = fork.merge_into(&mut tape).expect("合并成功");

        assert_eq!(merged_ids, vec![2, 3, 4]);
        assert_eq!(tape.entries().len(), 4);
        assert_eq!(
            tape.entries()[0].as_message().map(|value| value.content.clone()),
            Some("base".into())
        );
        assert_eq!(
            tape.entries()[1].as_message().map(|value| value.content.clone()),
            Some("branch reply".into())
        );
        assert_eq!(handoff.event_id, 4);
        assert_eq!(tape.entries()[2].kind, "anchor");
        assert_eq!(tape.entries()[3].kind, "event");
    }

    #[test]
    fn 条目工厂方法生成正确的_kind() {
        let msg = TapeEntry::message(&Message::new(Role::User, "hello"));
        assert_eq!(msg.kind, "message");

        let sys = TapeEntry::system("instructions");
        assert_eq!(sys.kind, "system");

        let anchor = TapeEntry::anchor("test", None);
        assert_eq!(anchor.kind, "anchor");

        let call = TapeEntry::tool_call(&ToolCall::new("search"));
        assert_eq!(call.kind, "tool_call");

        let result = TapeEntry::tool_result(&ToolResult::from_call(&ToolCall::new("search"), "ok"));
        assert_eq!(result.kind, "tool_result");

        let event = TapeEntry::event("started", None);
        assert_eq!(event.kind, "event");

        let error = TapeEntry::error("oops");
        assert_eq!(error.kind, "error");
    }

    #[test]
    fn with_meta_和_with_run_id_正确设置元数据() {
        let entry = TapeEntry::message(&Message::new(Role::User, "hello"))
            .with_run_id("turn-1")
            .with_meta("source_entry_ids", json!([1, 2, 3]));

        assert_eq!(entry.meta.get("run_id").and_then(|v| v.as_str()), Some("turn-1"));
        assert!(entry.meta.get("source_entry_ids").is_some());
    }

    #[test]
    fn now_iso8601_生成合法格式() {
        let ts = super::now_iso8601();
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
    }
}
