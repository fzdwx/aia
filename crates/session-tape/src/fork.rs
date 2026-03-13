use serde_json::Value;

use crate::{Handoff, SessionTape, SessionTapeError, TapeEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTapeFork {
    pub(crate) parent_name: String,
    pub(crate) base_len: usize,
    pub(crate) base_next_id: u64,
    pub(crate) tape: SessionTape,
}

impl SessionTapeFork {
    pub fn append(&mut self, message: agent_core::Message) -> u64 {
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
