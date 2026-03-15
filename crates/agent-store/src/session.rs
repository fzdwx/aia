use serde::{Deserialize, Serialize};

use crate::{AiaStore, AiaStoreError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
}

impl AiaStore {
    pub(crate) fn init_session_schema(&self) -> Result<(), AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
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
    }

    pub fn create_session(&self, record: &SessionRecord) -> Result<(), AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, model) VALUES (?1, ?2, ?3, ?4, ?5)",
            (&record.id, &record.title, &record.created_at, &record.updated_at, &record.model),
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, model FROM sessions ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                model: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, model FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                model: row.get(4)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_session(
        &self,
        id: &str,
        title: Option<&str>,
        model: Option<&str>,
    ) -> Result<bool, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let now = iso8601_now();
        let mut parts = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(t) = title {
            parts.push("title = ?");
            params.push(Box::new(t.to_string()));
        }
        if let Some(m) = model {
            parts.push("model = ?");
            params.push(Box::new(m.to_string()));
        }
        parts.push("updated_at = ?");
        params.push(Box::new(now));
        params.push(Box::new(id.to_string()));

        let set_clause = parts.join(", ");
        let sql = format!("UPDATE sessions SET {set_clause} WHERE id = ?");

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let changed = conn.execute(&sql, param_refs.as_slice())?;
        Ok(changed > 0)
    }

    pub fn delete_session(&self, id: &str) -> Result<bool, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let changed = conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(changed > 0)
    }
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
mod tests {
    use super::*;
    use crate::AiaStore;

    #[test]
    fn session_id_format_is_valid() {
        let id = generate_session_id();
        assert_eq!(id.len(), 17); // YYYYMMDD_XXXXXXXX
        assert_eq!(id.as_bytes()[8], b'_');
        assert!(id[9..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn crud_operations_work() {
        let store = AiaStore::in_memory().expect("store init");
        let now = iso8601_now();
        let record = SessionRecord {
            id: "20260315_abcd1234".to_string(),
            title: "Test session".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            model: "gpt-4.1".to_string(),
        };

        store.create_session(&record).expect("create");

        let sessions = store.list_sessions().expect("list");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "20260315_abcd1234");
        assert_eq!(sessions[0].title, "Test session");

        let found = store.get_session("20260315_abcd1234").expect("get");
        assert!(found.is_some());
        assert_eq!(found.as_ref().expect("some").model, "gpt-4.1");

        let updated =
            store.update_session("20260315_abcd1234", Some("New title"), None).expect("update");
        assert!(updated);
        let found = store.get_session("20260315_abcd1234").expect("get").expect("some");
        assert_eq!(found.title, "New title");

        let deleted = store.delete_session("20260315_abcd1234").expect("delete");
        assert!(deleted);
        let sessions = store.list_sessions().expect("list");
        assert!(sessions.is_empty());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let store = AiaStore::in_memory().expect("store init");
        let deleted = store.delete_session("nonexistent").expect("delete");
        assert!(!deleted);
    }

    #[test]
    fn iso8601_now_format_is_valid() {
        let ts = iso8601_now();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
    }
}
