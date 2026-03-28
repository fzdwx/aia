use rusqlite::Row;
use serde::{Deserialize, Serialize};
use rusqlite::Connection;

use std::sync::Arc;

use crate::{AiaStore, AiaStoreError};

const DEFAULT_SESSION_TITLE_LITERAL: &str = "New session";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTitleSource {
    Default,
    Auto,
    Manual,
    Channel,
}

impl SessionTitleSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Channel => "channel",
        }
    }

    fn parse(raw: &str) -> Result<Self, AiaStoreError> {
        match raw {
            "default" => Ok(Self::Default),
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            "channel" => Ok(Self::Channel),
            other => Err(AiaStoreError::new(format!(
                "unknown session title source: {other}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionAutoRenamePolicy {
    Enabled,
    Disabled,
    Inherit,
}

impl SessionAutoRenamePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Inherit => "inherit",
        }
    }

    fn parse(raw: &str) -> Result<Self, AiaStoreError> {
        match raw {
            "enabled" => Ok(Self::Enabled),
            "disabled" => Ok(Self::Disabled),
            "inherit" => Ok(Self::Inherit),
            other => Err(AiaStoreError::new(format!(
                "unknown session auto rename policy: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub title_source: SessionTitleSource,
    pub auto_rename_policy: SessionAutoRenamePolicy,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
    pub model: String,
}

impl SessionRecord {
    pub fn new(id: impl Into<String>, title: impl Into<String>, model: impl Into<String>) -> Self {
        let title = title.into();
        let title_source = default_title_source_for_title(&title);
        Self::new_with_metadata(
            id,
            title,
            model,
            title_source,
            SessionAutoRenamePolicy::Enabled,
        )
    }

    pub fn new_with_metadata(
        id: impl Into<String>,
        title: impl Into<String>,
        model: impl Into<String>,
        title_source: SessionTitleSource,
        auto_rename_policy: SessionAutoRenamePolicy,
    ) -> Self {
        let now = iso8601_now();
        Self {
            id: id.into(),
            title: title.into(),
            title_source,
            auto_rename_policy,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_active_at: now,
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
                    title_source TEXT NOT NULL DEFAULT 'default',
                    auto_rename_policy TEXT NOT NULL DEFAULT 'enabled',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL DEFAULT '',
                    user_turn_count_since_last_rename INTEGER NOT NULL DEFAULT 0,
                    rename_after_user_turns INTEGER NOT NULL DEFAULT 3,
                    model      TEXT NOT NULL DEFAULT ''
                );",
            )?;
            ensure_column(conn, "sessions", "title_source", "TEXT NOT NULL DEFAULT 'default'")?;
            ensure_column(
                conn,
                "sessions",
                "auto_rename_policy",
                "TEXT NOT NULL DEFAULT 'enabled'",
            )?;
            ensure_column(conn, "sessions", "last_active_at", "TEXT NOT NULL DEFAULT ''")?;
            ensure_column(
                conn,
                "sessions",
                "user_turn_count_since_last_rename",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "sessions",
                "rename_after_user_turns",
                "INTEGER NOT NULL DEFAULT 3",
            )?;
            backfill_session_columns(conn)?;
            Ok(())
        })
    }

    pub fn create_session(&self, record: &SessionRecord) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            let rename_after_user_turns = next_session_rename_threshold();
            conn.execute(
                "INSERT INTO sessions (id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, user_turn_count_since_last_rename, rename_after_user_turns, model) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                (
                    &record.id,
                    &record.title,
                    record.title_source.as_str(),
                    record.auto_rename_policy.as_str(),
                    &record.created_at,
                    &record.updated_at,
                    &record.last_active_at,
                    0_u32,
                    rename_after_user_turns,
                    &record.model,
                ),
            )?;
            Ok(())
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, AiaStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, model FROM sessions ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map([], read_session_record)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, AiaStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, model FROM sessions WHERE id = ?1",
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
            let rename_after_user_turns = next_session_rename_threshold();
            conn.execute(
                "INSERT INTO sessions (id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, user_turn_count_since_last_rename, rename_after_user_turns, model) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                (
                    &record.id,
                    &record.title,
                    record.title_source.as_str(),
                    record.auto_rename_policy.as_str(),
                    &record.created_at,
                    &record.updated_at,
                    &record.last_active_at,
                    0_u32,
                    rename_after_user_turns,
                    &record.model,
                ),
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
                "SELECT id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, model FROM sessions ORDER BY created_at ASC",
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
                "SELECT id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, model FROM sessions WHERE id = ?1",
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

    pub async fn touch_session_last_active_async(
        self: &Arc<Self>,
        id: impl Into<String>,
    ) -> Result<Option<SessionRecord>, AiaStoreError> {
        let id = id.into();
        let now = iso8601_now();
        self.with_conn_async(move |conn| {
            let changed = conn.execute(
                "UPDATE sessions SET updated_at = ?1, last_active_at = ?2 WHERE id = ?3",
                (now.as_str(), now.as_str(), id.as_str()),
            )?;
            if changed == 0 {
                return Ok(None);
            }

            let mut stmt = conn.prepare(
                "SELECT id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, model FROM sessions WHERE id = ?1",
            )?;
            let mut rows = stmt.query_map([id], read_session_record)?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn note_completed_user_turn_for_auto_rename_async(
        self: &Arc<Self>,
        id: impl Into<String>,
        allow_schedule: bool,
    ) -> Result<Option<SessionRecord>, AiaStoreError> {
        let id = id.into();
        self.with_conn_async(move |conn| {
            let Some(mut row) = load_session_row(conn, id.as_str())? else {
                return Ok(None);
            };

            if row.record.auto_rename_policy != SessionAutoRenamePolicy::Enabled {
                return Ok(None);
            }
            if !matches!(
                row.record.title_source,
                SessionTitleSource::Default | SessionTitleSource::Auto
            ) {
                return Ok(None);
            }

            row.user_turn_count_since_last_rename =
                row.user_turn_count_since_last_rename.saturating_add(1);
            if allow_schedule && row.user_turn_count_since_last_rename >= row.rename_after_user_turns {
                row.user_turn_count_since_last_rename = 0;
                row.rename_after_user_turns = next_session_rename_threshold();
                persist_session_auto_rename_state(conn, &row)?;
                return Ok(Some(row.record));
            }

            persist_session_auto_rename_state(conn, &row)?;
            Ok(None)
        })
        .await
    }

    pub async fn apply_auto_rename_title_async(
        self: &Arc<Self>,
        id: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<Option<SessionRecord>, AiaStoreError> {
        let id = id.into();
        let title = title.into();
        let now = iso8601_now();
        self.with_conn_async(move |conn| {
            let Some(mut row) = load_session_row(conn, id.as_str())? else {
                return Ok(None);
            };
            if row.record.title == title {
                return Ok(None);
            }
            if !matches!(
                row.record.title_source,
                SessionTitleSource::Default | SessionTitleSource::Auto
            ) {
                return Ok(None);
            }

            row.record.title = title;
            row.record.title_source = SessionTitleSource::Auto;
            row.record.updated_at = now;
            conn.execute(
                "UPDATE sessions SET title = ?1, title_source = ?2, updated_at = ?3 WHERE id = ?4",
                (
                    row.record.title.as_str(),
                    row.record.title_source.as_str(),
                    row.record.updated_at.as_str(),
                    row.record.id.as_str(),
                ),
            )?;
            Ok(Some(row.record))
        })
        .await
    }

}

fn read_session_record(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    let title_source = SessionTitleSource::parse(&row.get::<_, String>(2)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    let auto_rename_policy =
        SessionAutoRenamePolicy::parse(&row.get::<_, String>(3)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(SessionRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        title_source,
        auto_rename_policy,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        last_active_at: row.get(6)?,
        model: row.get(7)?,
    })
}

fn default_title_source_for_title(title: &str) -> SessionTitleSource {
    let normalized = title.trim();
    if normalized.is_empty() || normalized == DEFAULT_SESSION_TITLE_LITERAL {
        SessionTitleSource::Default
    } else {
        SessionTitleSource::Manual
    }
}

#[derive(Clone, Debug)]
struct SessionRow {
    record: SessionRecord,
    user_turn_count_since_last_rename: u32,
    rename_after_user_turns: u32,
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AiaStoreError> {
    if column_exists(conn, table, column)? {
        return Ok(());
    }

    let alter = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    conn.execute(&alter, [])?;
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, AiaStoreError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let existing =
        stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>()?;
    Ok(existing.iter().any(|name| name == column))
}

fn backfill_session_columns(conn: &Connection) -> Result<(), AiaStoreError> {
    conn.execute(
        "UPDATE sessions
         SET title_source = CASE
             WHEN TRIM(title) = '' OR title = 'New session' THEN 'default'
             ELSE 'manual'
         END
         WHERE title_source = ''
            OR (title_source = 'default' AND TRIM(title) <> '' AND title <> 'New session')",
        [],
    )?;
    conn.execute(
        "UPDATE sessions
         SET auto_rename_policy = 'enabled'
         WHERE auto_rename_policy = ''",
        [],
    )?;
    conn.execute(
        "UPDATE sessions
         SET last_active_at = updated_at
         WHERE last_active_at = ''",
        [],
    )?;
    conn.execute(
        "UPDATE sessions
         SET user_turn_count_since_last_rename = 0
         WHERE user_turn_count_since_last_rename < 0",
        [],
    )?;
    conn.execute(
        "UPDATE sessions
         SET rename_after_user_turns = 3
         WHERE rename_after_user_turns <= 0",
        [],
    )?;
    Ok(())
}

fn load_session_row(conn: &Connection, id: &str) -> Result<Option<SessionRow>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, title_source, auto_rename_policy, created_at, updated_at, last_active_at, model, user_turn_count_since_last_rename, rename_after_user_turns FROM sessions WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map([id], read_session_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

fn read_session_row(row: &Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        record: read_session_record(row)?,
        user_turn_count_since_last_rename: row.get(8)?,
        rename_after_user_turns: row.get(9)?,
    })
}

fn persist_session_auto_rename_state(
    conn: &Connection,
    row: &SessionRow,
) -> Result<(), AiaStoreError> {
    conn.execute(
        "UPDATE sessions SET user_turn_count_since_last_rename = ?1, rename_after_user_turns = ?2 WHERE id = ?3",
        (
            row.user_turn_count_since_last_rename,
            row.rename_after_user_turns,
            row.record.id.as_str(),
        ),
    )?;
    Ok(())
}

fn next_session_rename_threshold() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    3 + (nanos % 3)
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
