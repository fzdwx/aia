use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TapeQuery {
    pub(crate) after_entry_id: Option<u64>,
    pub(crate) after_latest_anchor: bool,
    pub(crate) after_anchor_name: Option<String>,
    pub(crate) before_anchor_name: Option<String>,
    pub(crate) start_date: Option<String>,
    pub(crate) end_date: Option<String>,
    pub(crate) kinds: BTreeSet<String>,
    pub(crate) text: Option<String>,
    pub(crate) limit: Option<usize>,
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
