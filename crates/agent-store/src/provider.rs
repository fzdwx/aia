use std::sync::Arc;

use provider_registry::ProviderRegistry;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::{AiaStore, AiaStoreError, iso8601_now};

const PROVIDER_REGISTRY_ROW_ID: i64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProviderRegistry {
    pub registry_json: String,
    pub updated_at: String,
}

impl StoredProviderRegistry {
    pub fn new(registry_json: impl Into<String>) -> Self {
        Self { registry_json: registry_json.into(), updated_at: iso8601_now() }
    }
}

impl AiaStore {
    pub(crate) fn init_provider_schema(&self) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS provider_registry (
                    id            INTEGER PRIMARY KEY CHECK (id = 1),
                    registry_json TEXT NOT NULL,
                    updated_at    TEXT NOT NULL
                );",
            )?;
            Ok(())
        })
    }

    pub fn load_provider_registry(&self) -> Result<ProviderRegistry, AiaStoreError> {
        self.with_conn(|conn| {
            let stored = conn
                .query_row(
                    "SELECT registry_json, updated_at FROM provider_registry WHERE id = ?1",
                    [PROVIDER_REGISTRY_ROW_ID],
                    |row| {
                        Ok(StoredProviderRegistry {
                            registry_json: row.get(0)?,
                            updated_at: row.get(1)?,
                        })
                    },
                )
                .optional()?;

            match stored {
                Some(stored) => {
                    serde_json::from_str(&stored.registry_json).map_err(AiaStoreError::from)
                }
                None => Ok(ProviderRegistry::default()),
            }
        })
    }

    pub fn save_provider_registry(&self, registry: &ProviderRegistry) -> Result<(), AiaStoreError> {
        let stored = StoredProviderRegistry::new(serde_json::to_string_pretty(registry)?);
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO provider_registry (id, registry_json, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET
                    registry_json = excluded.registry_json,
                    updated_at = excluded.updated_at",
                (
                    PROVIDER_REGISTRY_ROW_ID,
                    stored.registry_json.as_str(),
                    stored.updated_at.as_str(),
                ),
            )?;
            Ok(())
        })
    }

    pub async fn load_provider_registry_async(
        self: &Arc<Self>,
    ) -> Result<ProviderRegistry, AiaStoreError> {
        self.with_conn_async(move |conn| {
            let stored = conn
                .query_row(
                    "SELECT registry_json, updated_at FROM provider_registry WHERE id = ?1",
                    [PROVIDER_REGISTRY_ROW_ID],
                    |row| {
                        Ok(StoredProviderRegistry {
                            registry_json: row.get(0)?,
                            updated_at: row.get(1)?,
                        })
                    },
                )
                .optional()?;

            match stored {
                Some(stored) => {
                    serde_json::from_str(&stored.registry_json).map_err(AiaStoreError::from)
                }
                None => Ok(ProviderRegistry::default()),
            }
        })
        .await
    }

    pub async fn save_provider_registry_async(
        self: &Arc<Self>,
        registry: ProviderRegistry,
    ) -> Result<(), AiaStoreError> {
        let stored = StoredProviderRegistry::new(serde_json::to_string_pretty(&registry)?);
        self.with_conn_async(move |conn| {
            conn.execute(
                "INSERT INTO provider_registry (id, registry_json, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET
                    registry_json = excluded.registry_json,
                    updated_at = excluded.updated_at",
                (
                    PROVIDER_REGISTRY_ROW_ID,
                    stored.registry_json.as_str(),
                    stored.updated_at.as_str(),
                ),
            )?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
#[path = "../tests/provider/mod.rs"]
mod tests;
