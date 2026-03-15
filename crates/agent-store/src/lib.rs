mod session;
mod trace;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

pub use session::{SessionRecord, generate_session_id, iso8601_now};
pub use trace::{
    LlmTraceEvent, LlmTraceListItem, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus,
    LlmTraceStore, LlmTraceSummary,
};

#[derive(Debug)]
pub struct AiaStoreError {
    message: String,
}

impl AiaStoreError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl std::fmt::Display for AiaStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AiaStoreError {}

impl From<rusqlite::Error> for AiaStoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<serde_json::Error> for AiaStoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::new(value.to_string())
    }
}

/// Backward compatibility alias.
pub type LlmTraceStoreError = AiaStoreError;

/// Backward compatibility alias.
pub type SqliteLlmTraceStore = AiaStore;

pub struct AiaStore {
    pub(crate) conn: Mutex<Connection>,
}

impl AiaStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, AiaStoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AiaStoreError::new(format!("directory create failed: {e}")))?;
        }
        let conn = Connection::open(path).map_err(AiaStoreError::from)?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_trace_schema()?;
        store.init_session_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, AiaStoreError> {
        let conn = Connection::open_in_memory().map_err(AiaStoreError::from)?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_trace_schema()?;
        store.init_session_schema()?;
        Ok(store)
    }

    /// Migrate data from a legacy SQLite file by ATTACHing it and copying rows.
    pub fn migrate_from_legacy_file(
        &self,
        old_path: &Path,
        table_name: &str,
    ) -> Result<(), AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let path_str = old_path.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!(
            "ATTACH DATABASE '{path_str}' AS legacy;
             INSERT OR IGNORE INTO {table_name} SELECT * FROM legacy.{table_name};
             DETACH DATABASE legacy;",
        ))?;
        Ok(())
    }
}
