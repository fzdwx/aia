use std::{fs, fs::OpenOptions, io::Write, path::Path};

use std::collections::BTreeSet;

use agent_core::{ConversationItem, Message, QuestionRequest, QuestionResult};
use serde_json::Value;

use crate::entry::serialize_payload;
use crate::{
    Anchor, Handoff, SessionProviderBinding, SessionTapeError, SessionTapeFork, SessionView,
    TapeEntry, TapeQuery, anchor_from_entry, decode_persisted_line, project_conversation_item,
    project_message,
};

pub fn default_session_path() -> std::path::PathBuf {
    aia_config::default_session_tape_path()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTape {
    pub(crate) name: String,
    pub(crate) next_id: u64,
    pub(crate) entries: Vec<TapeEntry>,
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

    pub(crate) fn load_persisted_entry(
        &mut self,
        entry: TapeEntry,
    ) -> Result<u64, SessionTapeError> {
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
            Some(serialize_payload("provider_binding", &provider_binding)),
        ))
    }

    pub fn record_question_requested(&mut self, request: &QuestionRequest) -> u64 {
        self.append_entry(TapeEntry::event(
            "question_requested",
            Some(serialize_payload("question_requested", request)),
        ))
    }

    pub fn record_question_resolved(&mut self, result: &QuestionResult) -> u64 {
        self.append_entry(TapeEntry::event(
            "question_resolved",
            Some(serialize_payload("question_resolved", result)),
        ))
    }

    pub fn try_latest_provider_binding(
        &self,
    ) -> Result<Option<SessionProviderBinding>, SessionTapeError> {
        let Some(entry) =
            self.entries.iter().rev().find(|entry| {
                entry.kind == "event" && entry.event_name() == Some("provider_binding")
            })
        else {
            return Ok(None);
        };
        let data = entry.event_data().ok_or_else(|| {
            SessionTapeError::new("provider_binding 事件缺少 data 载荷".to_string())
        })?;
        serde_json::from_value(data.clone()).map(Some).map_err(|error| {
            SessionTapeError::new(format!("provider_binding 事件解码失败: {error}"))
        })
    }

    pub fn latest_provider_binding(&self) -> Option<SessionProviderBinding> {
        self.try_latest_provider_binding().ok().flatten()
    }

    pub fn try_pending_question_request(
        &self,
    ) -> Result<Option<QuestionRequest>, SessionTapeError> {
        let mut resolved_request_ids = BTreeSet::new();

        for entry in self.entries.iter().rev() {
            if entry.kind != "event" {
                continue;
            }

            match entry.event_name() {
                Some("question_resolved") => {
                    let data = entry.event_data().ok_or_else(|| {
                        SessionTapeError::new("question_resolved 事件缺少 data 载荷".to_string())
                    })?;
                    let result: QuestionResult =
                        serde_json::from_value(data.clone()).map_err(|error| {
                            SessionTapeError::new(format!(
                                "question_resolved 事件解码失败: {error}"
                            ))
                        })?;
                    resolved_request_ids.insert(result.request_id);
                }
                Some("question_requested") => {
                    let data = entry.event_data().ok_or_else(|| {
                        SessionTapeError::new("question_requested 事件缺少 data 载荷".to_string())
                    })?;
                    let request: QuestionRequest =
                        serde_json::from_value(data.clone()).map_err(|error| {
                            SessionTapeError::new(format!(
                                "question_requested 事件解码失败: {error}"
                            ))
                        })?;
                    if !resolved_request_ids.contains(&request.request_id) {
                        return Ok(Some(request));
                    }
                }
                _ => {}
            }
        }

        Ok(None)
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

    pub fn handoff_after_entry(
        &mut self,
        after_entry_id: u64,
        name: impl Into<String>,
        state: Value,
    ) -> Result<Handoff, SessionTapeError> {
        let insert_index = self
            .entries
            .iter()
            .position(|entry| entry.id == after_entry_id)
            .map(|index| index + 1)
            .ok_or_else(|| {
                SessionTapeError::new(format!("未找到插入锚点位置：{after_entry_id}"))
            })?;

        let name = name.into();
        let anchor_state = state;
        let anchor_id = self.next_id;
        self.next_id += 1;
        let mut anchor_entry = TapeEntry::anchor(&name, Some(anchor_state.clone()));
        anchor_entry.id = anchor_id;
        let event_id = self.next_id;
        self.next_id += 1;
        let mut event_entry =
            TapeEntry::event("handoff", Some(serde_json::json!({"anchor_entry_id": anchor_id})));
        event_entry.id = event_id;

        self.entries.insert(insert_index, anchor_entry);
        self.entries.insert(insert_index + 1, event_entry);

        Ok(Handoff { anchor: Anchor { entry_id: anchor_id, name, state: anchor_state }, event_id })
    }

    pub fn latest_anchor(&self) -> Option<Anchor> {
        self.entries.iter().rev().find_map(anchor_from_entry)
    }

    pub fn assemble_view(&self, anchor: Option<&Anchor>) -> SessionView {
        let lower_bound = anchor.map(|value| value.entry_id).unwrap_or(0);
        let entries =
            self.entries.iter().filter(|entry| entry.id > lower_bound).cloned().collect::<Vec<_>>();
        let messages = entries.iter().filter_map(project_message).collect::<Vec<_>>();
        let conversation = entries.iter().filter_map(project_conversation_item).collect::<Vec<_>>();

        SessionView { origin_anchor: anchor.cloned(), entries, messages, conversation }
    }

    pub fn view_from(&self, anchor: Option<&Anchor>) -> Vec<Message> {
        self.assemble_view(anchor).messages
    }

    pub fn default_view(&self) -> SessionView {
        let latest_anchor = self.latest_anchor();
        let Some(anchor) = latest_anchor.as_ref() else {
            return self.assemble_view(None);
        };

        let lower_bound = anchor
            .state
            .get("compressed_until_entry_id")
            .and_then(|value| value.as_u64())
            .unwrap_or(anchor.entry_id);
        self.assemble_view_with_lower_bound(Some(anchor), lower_bound)
    }

    pub fn default_messages(&self) -> Vec<Message> {
        self.default_view().messages
    }

    pub fn conversation_since(&self, entry_id: u64) -> Vec<ConversationItem> {
        self.entries
            .iter()
            .filter(|entry| entry.id > entry_id)
            .filter_map(project_conversation_item)
            .collect()
    }

    fn assemble_view_with_lower_bound(
        &self,
        anchor: Option<&Anchor>,
        lower_bound: u64,
    ) -> SessionView {
        let entries =
            self.entries.iter().filter(|entry| entry.id > lower_bound).cloned().collect::<Vec<_>>();
        let messages = entries.iter().filter_map(project_message).collect::<Vec<_>>();
        let conversation = entries.iter().filter_map(project_conversation_item).collect::<Vec<_>>();

        SessionView { origin_anchor: anchor.cloned(), entries, messages, conversation }
    }

    pub fn tape_name(&self) -> &str {
        &self.name
    }

    pub fn entries(&self) -> &[TapeEntry] {
        &self.entries
    }

    pub fn set_entry_meta(&mut self, entry_id: u64, key: &str, value: Value) {
        if let Some(entry) = self.entries.iter_mut().rev().find(|e| e.id == entry_id)
            && let Value::Object(ref mut map) = entry.meta
        {
            map.insert(key.to_string(), value);
        }
    }

    pub fn anchors(&self) -> Vec<Anchor> {
        self.entries.iter().filter_map(anchor_from_entry).collect()
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

    pub fn append_jsonl_entry(path: &Path, entry: &TapeEntry) -> Result<(), SessionTapeError> {
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
            .filter_map(anchor_from_entry)
            .find(|anchor| anchor.name == *anchor_name)
            .ok_or_else(|| SessionTapeError::new(format!("未找到锚点：{anchor_name}")))?;
        Ok(Some(anchor.entry_id))
    }

    fn find_anchor(&self, anchor_name: &str) -> Option<Anchor> {
        self.entries
            .iter()
            .rev()
            .filter_map(anchor_from_entry)
            .find(|anchor| anchor.name == anchor_name)
    }
}
