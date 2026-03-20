use rusqlite::Row;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::{AiaStore, AiaStoreError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
}

impl SessionRecord {
    pub fn new(id: impl Into<String>, title: impl Into<String>, model: impl Into<String>) -> Self {
        let now = iso8601_now();
        Self {
            id: id.into(),
            title: title.into(),
            created_at: now.clone(),
            updated_at: now,
            model: model.into(),
        }
    }
}

impl AiaStore {
    pub(crate) fn init_session_schema(&self) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sessions (
                    id         TEXT PRIMARY KEY,
                    title      TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    model      TEXT NOT NULL DEFAULT ''
                );",
            )?;
            Ok(())
        })
    }

    pub fn create_session(&self, record: &SessionRecord) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, model) VALUES (?1, ?2, ?3, ?4, ?5)",
                (&record.id, &record.title, &record.created_at, &record.updated_at, &record.model),
            )?;
            Ok(())
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, AiaStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, created_at, updated_at, model FROM sessions ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map([], read_session_record)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, AiaStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, created_at, updated_at, model FROM sessions WHERE id = ?1",
            )?;
            let mut rows = stmt.query_map([id], read_session_record)?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    pub fn first_session_id(&self) -> Result<Option<String>, AiaStoreError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM sessions ORDER BY created_at ASC LIMIT 1")?;
            let mut rows = stmt.query([])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get(0)?)),
                None => Ok(None),
            }
        })
    }

    pub fn update_session(
        &self,
        id: &str,
        title: Option<&str>,
        model: Option<&str>,
    ) -> Result<bool, AiaStoreError> {
        let now = iso8601_now();
        self.with_conn(|conn| {
            let changed = match (title, model) {
                (Some(title), Some(model)) => conn.execute(
                    "UPDATE sessions SET title = ?1, model = ?2, updated_at = ?3 WHERE id = ?4",
                    (title, model, now.as_str(), id),
                )?,
                (Some(title), None) => conn.execute(
                    "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    (title, now.as_str(), id),
                )?,
                (None, Some(model)) => conn.execute(
                    "UPDATE sessions SET model = ?1, updated_at = ?2 WHERE id = ?3",
                    (model, now.as_str(), id),
                )?,
                (None, None) => conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    (now.as_str(), id),
                )?,
            };
            Ok(changed > 0)
        })
    }

    pub fn delete_session(&self, id: &str) -> Result<bool, AiaStoreError> {
        self.with_conn(|conn| {
            let changed = conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
            Ok(changed > 0)
        })
    }

    pub async fn create_session_async(
        self: &Arc<Self>,
        record: SessionRecord,
    ) -> Result<(), AiaStoreError> {
        self.with_conn_async(move |conn| {
            conn.execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, model) VALUES (?1, ?2, ?3, ?4, ?5)",
                (&record.id, &record.title, &record.created_at, &record.updated_at, &record.model),
            )?;
            Ok(())
        })
        .await
    }

    pub async fn list_sessions_async(
        self: &Arc<Self>,
    ) -> Result<Vec<SessionRecord>, AiaStoreError> {
        self.with_conn_async(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, created_at, updated_at, model FROM sessions ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map([], read_session_record)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
        })
        .await
    }

    pub async fn get_session_async(
        self: &Arc<Self>,
        id: impl Into<String>,
    ) -> Result<Option<SessionRecord>, AiaStoreError> {
        let id = id.into();
        self.with_conn_async(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, created_at, updated_at, model FROM sessions WHERE id = ?1",
            )?;
            let mut rows = stmt.query_map([id], read_session_record)?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn first_session_id_async(self: &Arc<Self>) -> Result<Option<String>, AiaStoreError> {
        self.with_conn_async(move |conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM sessions ORDER BY created_at ASC LIMIT 1")?;
            let mut rows = stmt.query([])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get(0)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn update_session_async(
        self: &Arc<Self>,
        id: impl Into<String>,
        title: Option<String>,
        model: Option<String>,
    ) -> Result<bool, AiaStoreError> {
        let id = id.into();
        let now = iso8601_now();
        self.with_conn_async(move |conn| {
            let changed = match (title.as_deref(), model.as_deref()) {
                (Some(title), Some(model)) => conn.execute(
                    "UPDATE sessions SET title = ?1, model = ?2, updated_at = ?3 WHERE id = ?4",
                    (title, model, now.as_str(), id.as_str()),
                )?,
                (Some(title), None) => conn.execute(
                    "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    (title, now.as_str(), id.as_str()),
                )?,
                (None, Some(model)) => conn.execute(
                    "UPDATE sessions SET model = ?1, updated_at = ?2 WHERE id = ?3",
                    (model, now.as_str(), id.as_str()),
                )?,
                (None, None) => conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    (now.as_str(), id.as_str()),
                )?,
            };
            Ok(changed > 0)
        })
        .await
    }

    pub async fn delete_session_async(
        self: &Arc<Self>,
        id: impl Into<String>,
    ) -> Result<bool, AiaStoreError> {
        let id = id.into();
        self.with_conn_async(move |conn| {
            let changed = conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
            Ok(changed > 0)
        })
        .await
    }
}

fn read_session_record(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        model: row.get(4)?,
    })
}

/// Generate a session ID: `YYYYMMDD_8hexrandom`
pub fn generate_session_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = now / 86400;
    let (year, month, day) = days_to_ymd(days);
    let random_hex = random_hex_8();
    format!("{year:04}{month:02}{day:02}_{random_hex}")
}

pub fn iso8601_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = now / 86400;
    let day_seconds = now % 86400;
    let (year, month, day) = days_to_ymd(days);
    let hour = day_seconds / 3600;
    let minute = (day_seconds % 3600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn days_to_ymd(days_since_epoch: u64) -> (u64, u64, u64) {
    // Civil days algorithm (Howard Hinnant)
    let z = days_since_epoch + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn random_hex_8() -> String {
    let mut bytes = [0u8; 4];
    #[cfg(target_family = "unix")]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut bytes);
        }
    }
    #[cfg(not(target_family = "unix"))]
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        bytes = (t as u32).to_le_bytes();
    }
    format!("{:02x}{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2], bytes[3])
}

#[cfg(test)]
#[path = "../tests/session/mod.rs"]
mod tests;
