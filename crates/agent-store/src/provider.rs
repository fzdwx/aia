use std::collections::HashMap;
use std::sync::Arc;

use provider_registry::{
    AdapterKind, CredentialRef, ModelConfig, ModelLimit, ProviderAccount, ProviderEndpoint,
    ProviderRegistry,
};
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

use crate::{AiaStore, AiaStoreError, iso8601_now};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProviderAccount {
    pub id: String,
    pub label: String,
    pub adapter: String,
    pub base_url: String,
    pub api_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredProviderModel {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub context_limit: Option<u32>,
    pub output_limit: Option<u32>,
    pub default_temperature: Option<f32>,
    pub supports_reasoning: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl StoredProviderAccount {
    fn new(account: &ProviderAccount, created_at: Option<&str>) -> Self {
        let now = iso8601_now();
        Self {
            id: account.id.clone(),
            label: account.label.clone(),
            adapter: account.adapter.protocol_name().to_string(),
            base_url: account.endpoint.base_url.clone(),
            api_key: account.credential.api_key_value().to_string(),
            created_at: created_at.map(str::to_string).unwrap_or_else(|| now.clone()),
            updated_at: now,
        }
    }
}

impl StoredProviderModel {
    fn new(provider_id: &str, model: &ModelConfig, created_at: Option<&str>) -> Self {
        let now = iso8601_now();
        Self {
            provider_id: provider_id.to_string(),
            model_id: model.id.clone(),
            display_name: model.display_name.clone(),
            context_limit: model.limit.as_ref().and_then(|limit| limit.context),
            output_limit: model.limit.as_ref().and_then(|limit| limit.output),
            default_temperature: model.default_temperature,
            supports_reasoning: model.supports_reasoning,
            created_at: created_at.map(str::to_string).unwrap_or_else(|| now.clone()),
            updated_at: now,
        }
    }
}

impl AiaStore {
    pub(crate) fn init_provider_schema(&self) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS providers (
                    id           TEXT PRIMARY KEY,
                    label        TEXT NOT NULL,
                    adapter      TEXT NOT NULL,
                    base_url     TEXT NOT NULL,
                    api_key      TEXT NOT NULL,
                    created_at   TEXT NOT NULL,
                    updated_at   TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS provider_models (
                    provider_id         TEXT NOT NULL,
                    model_id            TEXT NOT NULL,
                    display_name        TEXT,
                    context_limit       INTEGER,
                    output_limit        INTEGER,
                    default_temperature REAL,
                    supports_reasoning  INTEGER NOT NULL,
                    created_at          TEXT NOT NULL,
                    updated_at          TEXT NOT NULL,
                    PRIMARY KEY (provider_id, model_id),
                    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_provider_models_provider_id
                    ON provider_models(provider_id);",
            )?;
            Ok(())
        })
    }

    pub fn load_provider_registry(&self) -> Result<ProviderRegistry, AiaStoreError> {
        self.with_conn(load_provider_registry_from_conn)
    }

    pub fn save_provider_registry(&self, registry: &ProviderRegistry) -> Result<(), AiaStoreError> {
        let registry = registry.clone();
        self.with_conn(move |conn| save_provider_registry_to_conn(conn, &registry))
    }

    pub async fn load_provider_registry_async(
        self: &Arc<Self>,
    ) -> Result<ProviderRegistry, AiaStoreError> {
        self.with_conn_async(load_provider_registry_from_conn).await
    }

    pub async fn save_provider_registry_async(
        self: &Arc<Self>,
        registry: ProviderRegistry,
    ) -> Result<(), AiaStoreError> {
        self.with_conn_async(move |conn| save_provider_registry_to_conn(conn, &registry)).await
    }
}

fn load_provider_registry_from_conn(
    conn: &rusqlite::Connection,
) -> Result<ProviderRegistry, AiaStoreError> {
    let profiles = {
        let mut stmt = conn.prepare(
            "SELECT id, label, adapter, base_url, api_key, created_at, updated_at
             FROM providers
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], read_provider_account_row)?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let models = {
        let mut stmt = conn.prepare(
            "SELECT provider_id, model_id, display_name, context_limit, output_limit,
                    default_temperature, supports_reasoning, created_at, updated_at
             FROM provider_models
             ORDER BY provider_id ASC, created_at DESC, rowid DESC, model_id ASC",
        )?;
        let rows = stmt.query_map([], read_provider_model_row)?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let providers = profiles
        .into_iter()
        .map(|account| {
            let provider_models = models
                .iter()
                .filter(|model| model.provider_id == account.id)
                .map(|model| ModelConfig {
                    id: model.model_id.clone(),
                    display_name: model.display_name.clone(),
                    limit: match (model.context_limit, model.output_limit) {
                        (None, None) => None,
                        (context, output) => Some(ModelLimit { context, output }),
                    },
                    default_temperature: model.default_temperature,
                    supports_reasoning: model.supports_reasoning,
                })
                .collect::<Vec<_>>();

            Ok(ProviderAccount {
                id: account.id,
                label: account.label,
                adapter: parse_adapter_kind(&account.adapter)?,
                endpoint: ProviderEndpoint { base_url: account.base_url },
                credential: CredentialRef::api_key(account.api_key),
                models: provider_models,
            })
        })
        .collect::<Result<Vec<_>, AiaStoreError>>()?;

    let mut registry = ProviderRegistry::default();
    for provider in providers {
        registry.upsert(provider);
    }
    Ok(registry)
}

fn save_provider_registry_to_conn(
    conn: &rusqlite::Connection,
    registry: &ProviderRegistry,
) -> Result<(), AiaStoreError> {
    let existing_profiles = load_existing_provider_profiles(conn)?
        .into_iter()
        .map(|profile| (profile.id.clone(), profile))
        .collect::<HashMap<_, _>>();
    let existing_models = load_existing_provider_models(conn)?
        .into_iter()
        .map(|model| ((model.provider_id.clone(), model.model_id.clone()), model))
        .collect::<HashMap<_, _>>();

    conn.execute("DELETE FROM provider_models", [])?;
    conn.execute("DELETE FROM providers", [])?;

    for account in registry.providers() {
        let stored_profile = StoredProviderAccount::new(
            account,
            existing_profiles.get(&account.id).map(|stored| stored.created_at.as_str()),
        );
        conn.execute(
            "INSERT INTO providers (id, label, adapter, base_url, api_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                stored_profile.id.as_str(),
                stored_profile.label.as_str(),
                stored_profile.adapter.as_str(),
                stored_profile.base_url.as_str(),
                stored_profile.api_key.as_str(),
                stored_profile.created_at.as_str(),
                stored_profile.updated_at.as_str(),
            ),
        )?;

        for model in account.models.iter().rev() {
            let stored_model = StoredProviderModel::new(
                &account.id,
                model,
                existing_models
                    .get(&(account.id.clone(), model.id.clone()))
                    .map(|stored| stored.created_at.as_str()),
            );
            conn.execute(
                "INSERT INTO provider_models (
                    provider_id, model_id, display_name, context_limit, output_limit,
                    default_temperature, supports_reasoning, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                (
                    stored_model.provider_id.as_str(),
                    stored_model.model_id.as_str(),
                    stored_model.display_name.as_deref(),
                    stored_model.context_limit,
                    stored_model.output_limit,
                    stored_model.default_temperature,
                    stored_model.supports_reasoning,
                    stored_model.created_at.as_str(),
                    stored_model.updated_at.as_str(),
                ),
            )?;
        }
    }

    Ok(())
}

fn load_existing_provider_profiles(
    conn: &Connection,
) -> Result<Vec<StoredProviderAccount>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, label, adapter, base_url, api_key, created_at, updated_at
         FROM providers
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], read_provider_account_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
}

fn load_existing_provider_models(
    conn: &Connection,
) -> Result<Vec<StoredProviderModel>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, model_id, display_name, context_limit, output_limit,
                default_temperature, supports_reasoning, created_at, updated_at
         FROM provider_models
         ORDER BY provider_id ASC, created_at DESC, rowid DESC, model_id ASC",
    )?;
    let rows = stmt.query_map([], read_provider_model_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
}

fn read_provider_account_row(row: &Row<'_>) -> rusqlite::Result<StoredProviderAccount> {
    Ok(StoredProviderAccount {
        id: row.get(0)?,
        label: row.get(1)?,
        adapter: row.get(2)?,
        base_url: row.get(3)?,
        api_key: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn read_provider_model_row(row: &Row<'_>) -> rusqlite::Result<StoredProviderModel> {
    Ok(StoredProviderModel {
        provider_id: row.get(0)?,
        model_id: row.get(1)?,
        display_name: row.get(2)?,
        context_limit: row.get(3)?,
        output_limit: row.get(4)?,
        default_temperature: row.get(5)?,
        supports_reasoning: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn parse_adapter_kind(kind: &str) -> Result<AdapterKind, AiaStoreError> {
    match kind {
        "openai-responses" => Ok(AdapterKind::OpenAiResponses),
        "openai-chat-completions" => Ok(AdapterKind::OpenAiChatCompletions),
        other => Err(AiaStoreError::new(format!("unknown provider kind: {other}"))),
    }
}

#[cfg(test)]
#[path = "../tests/provider/mod.rs"]
mod tests;
