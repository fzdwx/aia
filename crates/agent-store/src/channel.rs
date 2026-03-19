use rusqlite::Row;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::{AiaStore, AiaStoreError, iso8601_now};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalConversationKey {
    pub channel_kind: String,
    pub profile_id: String,
    pub scope: String,
    pub conversation_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelSessionBinding {
    pub channel_kind: String,
    pub profile_id: String,
    pub scope: String,
    pub conversation_key: String,
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelMessageReceipt {
    pub channel_kind: String,
    pub profile_id: String,
    pub external_message_id: String,
    pub session_id: String,
    pub created_at: String,
}

impl ChannelSessionBinding {
    pub fn new(key: ExternalConversationKey, session_id: impl Into<String>) -> Self {
        let now = iso8601_now();
        Self {
            channel_kind: key.channel_kind,
            profile_id: key.profile_id,
            scope: key.scope,
            conversation_key: key.conversation_key,
            session_id: session_id.into(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

impl ChannelMessageReceipt {
    pub fn new(
        channel_kind: impl Into<String>,
        profile_id: impl Into<String>,
        external_message_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            channel_kind: channel_kind.into(),
            profile_id: profile_id.into(),
            external_message_id: external_message_id.into(),
            session_id: session_id.into(),
            created_at: iso8601_now(),
        }
    }
}

impl AiaStore {
    pub(crate) fn init_channel_schema(&self) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS channel_session_bindings (
                    channel_kind     TEXT NOT NULL,
                    profile_id       TEXT NOT NULL,
                    scope            TEXT NOT NULL,
                    conversation_key TEXT NOT NULL,
                    session_id       TEXT NOT NULL,
                    created_at       TEXT NOT NULL,
                    updated_at       TEXT NOT NULL,
                    PRIMARY KEY (channel_kind, profile_id, scope, conversation_key)
                );
                CREATE TABLE IF NOT EXISTS channel_message_receipts (
                    channel_kind        TEXT NOT NULL,
                    profile_id          TEXT NOT NULL,
                    external_message_id TEXT NOT NULL,
                    session_id          TEXT NOT NULL,
                    created_at          TEXT NOT NULL,
                    PRIMARY KEY (channel_kind, profile_id, external_message_id)
                );",
            )?;
            Ok(())
        })
    }

    pub fn get_channel_binding(
        &self,
        key: &ExternalConversationKey,
    ) -> Result<Option<ChannelSessionBinding>, AiaStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT channel_kind, profile_id, scope, conversation_key, session_id, created_at, updated_at
                 FROM channel_session_bindings
                 WHERE channel_kind = ?1 AND profile_id = ?2 AND scope = ?3 AND conversation_key = ?4",
            )?;
            let mut rows = stmt.query_map(
                (
                    key.channel_kind.as_str(),
                    key.profile_id.as_str(),
                    key.scope.as_str(),
                    key.conversation_key.as_str(),
                ),
                read_channel_binding,
            )?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    pub fn upsert_channel_binding(
        &self,
        binding: &ChannelSessionBinding,
    ) -> Result<(), AiaStoreError> {
        let now = iso8601_now();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO channel_session_bindings
                 (channel_kind, profile_id, scope, conversation_key, session_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(channel_kind, profile_id, scope, conversation_key)
                 DO UPDATE SET session_id = excluded.session_id, updated_at = excluded.updated_at",
                (
                    binding.channel_kind.as_str(),
                    binding.profile_id.as_str(),
                    binding.scope.as_str(),
                    binding.conversation_key.as_str(),
                    binding.session_id.as_str(),
                    binding.created_at.as_str(),
                    now.as_str(),
                ),
            )?;
            Ok(())
        })
    }

    pub fn record_channel_message_receipt(
        &self,
        receipt: &ChannelMessageReceipt,
    ) -> Result<bool, AiaStoreError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "INSERT OR IGNORE INTO channel_message_receipts
                 (channel_kind, profile_id, external_message_id, session_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    receipt.channel_kind.as_str(),
                    receipt.profile_id.as_str(),
                    receipt.external_message_id.as_str(),
                    receipt.session_id.as_str(),
                    receipt.created_at.as_str(),
                ),
            )?;
            Ok(changed > 0)
        })
    }

    pub async fn get_channel_binding_async(
        self: &Arc<Self>,
        key: ExternalConversationKey,
    ) -> Result<Option<ChannelSessionBinding>, AiaStoreError> {
        self.with_conn_async(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT channel_kind, profile_id, scope, conversation_key, session_id, created_at, updated_at
                 FROM channel_session_bindings
                 WHERE channel_kind = ?1 AND profile_id = ?2 AND scope = ?3 AND conversation_key = ?4",
            )?;
            let mut rows = stmt.query_map(
                (
                    key.channel_kind.as_str(),
                    key.profile_id.as_str(),
                    key.scope.as_str(),
                    key.conversation_key.as_str(),
                ),
                read_channel_binding,
            )?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn upsert_channel_binding_async(
        self: &Arc<Self>,
        binding: ChannelSessionBinding,
    ) -> Result<(), AiaStoreError> {
        self.with_conn_async(move |conn| {
            let now = iso8601_now();
            conn.execute(
                "INSERT INTO channel_session_bindings
                 (channel_kind, profile_id, scope, conversation_key, session_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(channel_kind, profile_id, scope, conversation_key)
                 DO UPDATE SET session_id = excluded.session_id, updated_at = excluded.updated_at",
                (
                    binding.channel_kind.as_str(),
                    binding.profile_id.as_str(),
                    binding.scope.as_str(),
                    binding.conversation_key.as_str(),
                    binding.session_id.as_str(),
                    binding.created_at.as_str(),
                    now.as_str(),
                ),
            )?;
            Ok(())
        })
        .await
    }

    pub async fn delete_channel_bindings_by_session_id_async(
        self: &Arc<Self>,
        session_id: impl Into<String>,
    ) -> Result<usize, AiaStoreError> {
        let session_id = session_id.into();
        self.with_conn_async(move |conn| {
            let changed = conn.execute(
                "DELETE FROM channel_session_bindings WHERE session_id = ?1",
                [session_id],
            )?;
            Ok(changed)
        })
        .await
    }

    pub async fn record_channel_message_receipt_async(
        self: &Arc<Self>,
        receipt: ChannelMessageReceipt,
    ) -> Result<bool, AiaStoreError> {
        self.with_conn_async(move |conn| {
            let changed = conn.execute(
                "INSERT OR IGNORE INTO channel_message_receipts
                 (channel_kind, profile_id, external_message_id, session_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    receipt.channel_kind.as_str(),
                    receipt.profile_id.as_str(),
                    receipt.external_message_id.as_str(),
                    receipt.session_id.as_str(),
                    receipt.created_at.as_str(),
                ),
            )?;
            Ok(changed > 0)
        })
        .await
    }
}

fn read_channel_binding(row: &Row<'_>) -> rusqlite::Result<ChannelSessionBinding> {
    Ok(ChannelSessionBinding {
        channel_kind: row.get(0)?,
        profile_id: row.get(1)?,
        scope: row.get(2)?,
        conversation_key: row.get(3)?,
        session_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn sample_key() -> ExternalConversationKey {
        ExternalConversationKey {
            channel_kind: "feishu".into(),
            profile_id: "default".into(),
            scope: "group".into(),
            conversation_key: "chat:oc_123".into(),
        }
    }

    #[test]
    fn channel_binding_round_trip_works() {
        let store = AiaStore::in_memory().expect("store init");
        let key = sample_key();
        let binding = ChannelSessionBinding::new(key.clone(), "session-1");

        store.upsert_channel_binding(&binding).expect("binding should save");
        let found = store.get_channel_binding(&key).expect("binding should load");

        assert_eq!(found, Some(binding));
    }

    #[test]
    fn duplicate_message_receipt_is_ignored() {
        let store = AiaStore::in_memory().expect("store init");
        let receipt = ChannelMessageReceipt::new("feishu", "default", "om_123", "session-1");

        let first =
            store.record_channel_message_receipt(&receipt).expect("first receipt should save");
        let second = store
            .record_channel_message_receipt(&receipt)
            .expect("duplicate receipt should be handled");

        assert!(first);
        assert!(!second);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_channel_binding_round_trip_works() {
        let store = Arc::new(AiaStore::in_memory().expect("store init"));
        let key = sample_key();
        let binding = ChannelSessionBinding::new(key.clone(), "session-2");

        store
            .upsert_channel_binding_async(binding.clone())
            .await
            .expect("binding should save async");
        let found = store.get_channel_binding_async(key).await.expect("binding should load async");

        assert_eq!(found, Some(binding));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_channel_bindings_by_session_id_removes_binding() {
        let store = Arc::new(AiaStore::in_memory().expect("store init"));
        let key = sample_key();
        let binding = ChannelSessionBinding::new(key.clone(), "session-stale");

        store.upsert_channel_binding_async(binding).await.expect("binding should save async");

        let deleted = store
            .delete_channel_bindings_by_session_id_async("session-stale")
            .await
            .expect("delete bindings should succeed");
        let found = store.get_channel_binding_async(key).await.expect("binding should load async");

        assert_eq!(deleted, 1);
        assert_eq!(found, None);
    }
}
