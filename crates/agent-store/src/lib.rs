mod channel;
mod provider;
mod session;
mod trace;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

pub use channel::{
    ChannelMessageReceipt, ChannelSessionBinding, ExternalConversationKey, StoredChannelProfile,
};
pub use session::{SessionRecord, generate_session_id, iso8601_now};
pub use trace::{
    LlmTraceDashboard, LlmTraceDashboardActivityPoint, LlmTraceDashboardRange,
    LlmTraceDashboardSummary, LlmTraceDashboardTrendPoint, LlmTraceEvent, LlmTraceListItem,
    LlmTraceLoopDetail, LlmTraceLoopItem, LlmTraceLoopPage, LlmTraceLoopStatus, LlmTraceOverview,
    LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore, LlmTraceSummary,
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
        store.init_channel_schema()?;
        store.init_provider_schema()?;
        store.init_trace_schema()?;
        store.init_session_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, AiaStoreError> {
        let conn = Connection::open_in_memory().map_err(AiaStoreError::from)?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_channel_schema()?;
        store.init_provider_schema()?;
        store.init_trace_schema()?;
        store.init_session_schema()?;
        Ok(store)
    }

    fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn with_conn<R>(
        &self,
        action: impl FnOnce(&Connection) -> Result<R, AiaStoreError>,
    ) -> Result<R, AiaStoreError> {
        let conn = self.lock_conn();
        action(&conn)
    }

    pub(crate) async fn with_conn_async<R, F>(
        self: &Arc<Self>,
        action: F,
    ) -> Result<R, AiaStoreError>
    where
        R: Send + 'static,
        F: FnOnce(&Connection) -> Result<R, AiaStoreError> + Send + 'static,
    {
        let store = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_conn();
            action(&conn)
        })
        .await
        .map_err(|error| AiaStoreError::new(format!("store task join failed: {error}")))?
    }

    /// Migrate data from a legacy SQLite file by ATTACHing it and copying rows.
    pub fn migrate_from_legacy_file(
        &self,
        old_path: &Path,
        table_name: &str,
    ) -> Result<(), AiaStoreError> {
        let path_str = old_path.to_string_lossy().replace('\'', "''");
        self.with_conn(|conn| {
            conn.execute_batch(&format!(
                "ATTACH DATABASE '{path_str}' AS legacy;
                 INSERT OR IGNORE INTO {table_name} SELECT * FROM legacy.{table_name};
                 DETACH DATABASE legacy;",
            ))?;
            Ok(())
        })
    }
}
