use std::{fs, path::Path};

use agent_core::{ConversationItem, Message, ModelCheckpoint};
use serde_json::Value;

use crate::{
    Anchor, Handoff, SessionProviderBinding, SessionTapeError, SessionTapeFork, SessionView,
    StoredModelCheckpoint, TapeEntry, TapeQuery, anchor_from_entry, decode_persisted_line,
    project_conversation_item, project_message,
};

pub fn default_session_path() -> std::path::PathBuf {
    std::path::PathBuf::from(".aia/session.jsonl")
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

    pub fn record_model_checkpoint(
        &mut self,
        checkpoint: &ModelCheckpoint,
        checkpoint_entry_id: u64,
        run_id: &str,
    ) -> u64 {
        self.append_entry(
            TapeEntry::event(
                "model_checkpoint",
                Some(
                    serde_json::to_value(StoredModelCheckpoint {
                        checkpoint: checkpoint.clone(),
                        checkpoint_entry_id,
                    })
                    .unwrap_or_default(),
                ),
            )
            .with_run_id(run_id),
        )
    }

    pub fn latest_model_checkpoint(&self) -> Option<StoredModelCheckpoint> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| entry.kind == "event")
            .filter(|entry| entry.event_name() == Some("model_checkpoint"))
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
        self.assemble_view(latest_anchor.as_ref())
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

    pub fn tape_name(&self) -> &str {
        &self.name
    }

    pub fn entries(&self) -> &[TapeEntry] {
        &self.entries
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
