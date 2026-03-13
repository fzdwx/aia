mod binding;
mod compat;
mod entry;
mod error;
mod fork;
mod query;
mod storage;
mod tape;
mod view;

#[cfg(test)]
mod tests;

pub use binding::SessionProviderBinding;
pub(crate) use compat::decode_persisted_line;
pub use entry::TapeEntry;
pub(crate) use entry::default_meta;
pub use error::SessionTapeError;
pub use fork::SessionTapeFork;
pub use query::TapeQuery;
pub use storage::{InMemoryTapeStorage, JsonlTapeStorage, NamedTapeStorage};
pub use tape::{SessionTape, default_session_path};
pub use view::{Anchor, Handoff, SessionView, StoredModelCheckpoint};
pub(crate) use view::{anchor_from_entry, project_conversation_item, project_message};
