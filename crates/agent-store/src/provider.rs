use std::collections::HashMap;
use std::sync::Arc;

use provider_registry::{ModelConfig, ModelLimit, ProviderKind, ProviderProfile, ProviderRegistry};
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

use crate::{AiaStore, AiaStoreError, iso8601_now};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProviderProfile {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredProviderModel {
    pub provider_name: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub context_limit: Option<u32>,
    pub output_limit: Option<u32>,
    pub default_temperature: Option<f32>,
    pub supports_reasoning: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl StoredProviderProfile {
    fn new(profile: &ProviderProfile, is_active: bool, created_at: Option<&str>) -> Self {
        let now = iso8601_now();
        Self {
            name: profile.name.clone(),
            kind: profile.kind.protocol_name().to_string(),
            base_url: profile.base_url.clone(),
            api_key: profile.api_key.clone(),
            is_active,
            created_at: created_at.map(str::to_string).unwrap_or_else(|| now.clone()),
            updated_at: now,
        }
    }
}

impl StoredProviderModel {
    fn new(provider_name: &str, model: &ModelConfig, created_at: Option<&str>) -> Self {
        let now = iso8601_now();
        Self {
            provider_name: provider_name.to_string(),
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
                    name         TEXT PRIMARY KEY,
                    kind         TEXT NOT NULL,
                    base_url     TEXT NOT NULL,
                    api_key      TEXT NOT NULL,
                    is_active    INTEGER NOT NULL DEFAULT 0,
                    created_at   TEXT NOT NULL,
                    updated_at   TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS provider_models (
                    provider_name       TEXT NOT NULL,
                    model_id            TEXT NOT NULL,
                    display_name        TEXT,
                    context_limit       INTEGER,
                    output_limit        INTEGER,
                    default_temperature REAL,
                    supports_reasoning  INTEGER NOT NULL,
                    created_at          TEXT NOT NULL,
                    updated_at          TEXT NOT NULL,
                    PRIMARY KEY (provider_name, model_id),
                    FOREIGN KEY (provider_name) REFERENCES providers(name) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_provider_models_provider_name
                    ON provider_models(provider_name);",
            )?;
            ensure_column(conn, "providers", "is_active", "INTEGER NOT NULL DEFAULT 0")?;
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
            "SELECT name, kind, base_url, api_key, is_active, created_at, updated_at
             FROM providers
             ORDER BY name ASC",
        )?;
        let rows = stmt.query_map([], read_provider_profile_row)?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let models = {
        let mut stmt = conn.prepare(
            "SELECT provider_name, model_id, display_name, context_limit, output_limit,
                    default_temperature, supports_reasoning, created_at, updated_at
             FROM provider_models
             ORDER BY provider_name ASC, created_at DESC, rowid DESC, model_id ASC",
        )?;
        let rows = stmt.query_map([], read_provider_model_row)?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let active_provider_name =
        profiles.iter().find(|profile| profile.is_active).map(|profile| profile.name.clone());

    let providers = profiles
        .into_iter()
        .map(|profile| {
            let provider_models = models
                .iter()
                .filter(|model| model.provider_name == profile.name)
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

            Ok(ProviderProfile {
                name: profile.name,
                kind: parse_provider_kind(&profile.kind)?,
                base_url: profile.base_url,
                api_key: profile.api_key,
                models: provider_models,
            })
        })
        .collect::<Result<Vec<_>, AiaStoreError>>()?;

    let mut registry = ProviderRegistry::default();
    for provider in providers {
        registry.upsert(provider);
    }
    if let Some(active_provider_name) = active_provider_name {
        registry
            .set_active(&active_provider_name)
            .map_err(|error| AiaStoreError::new(error.to_string()))?;
    }
    Ok(registry)
}

fn save_provider_registry_to_conn(
    conn: &rusqlite::Connection,
    registry: &ProviderRegistry,
) -> Result<(), AiaStoreError> {
    let existing_profiles = load_existing_provider_profiles(conn)?
        .into_iter()
        .map(|profile| (profile.name.clone(), profile))
        .collect::<HashMap<_, _>>();
    let existing_models = load_existing_provider_models(conn)?
        .into_iter()
        .map(|model| ((model.provider_name.clone(), model.model_id.clone()), model))
        .collect::<HashMap<_, _>>();
    let active_provider_name = registry.active_provider().map(|provider| provider.name.as_str());

    conn.execute("DELETE FROM provider_models", [])?;
    conn.execute("DELETE FROM providers", [])?;

    for profile in registry.providers() {
        let stored_profile = StoredProviderProfile::new(
            profile,
            active_provider_name == Some(profile.name.as_str()),
            existing_profiles.get(&profile.name).map(|stored| stored.created_at.as_str()),
        );
        conn.execute(
            "INSERT INTO providers (name, kind, base_url, api_key, is_active, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                stored_profile.name.as_str(),
                stored_profile.kind.as_str(),
                stored_profile.base_url.as_str(),
                stored_profile.api_key.as_str(),
                stored_profile.is_active,
                stored_profile.created_at.as_str(),
                stored_profile.updated_at.as_str(),
            ),
        )?;

        for model in profile.models.iter().rev() {
            let stored_model = StoredProviderModel::new(
                &profile.name,
                model,
                existing_models
                    .get(&(profile.name.clone(), model.id.clone()))
                    .map(|stored| stored.created_at.as_str()),
            );
            conn.execute(
                "INSERT INTO provider_models (
                    provider_name, model_id, display_name, context_limit, output_limit,
                    default_temperature, supports_reasoning, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                (
                    stored_model.provider_name.as_str(),
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
) -> Result<Vec<StoredProviderProfile>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "SELECT name, kind, base_url, api_key, is_active, created_at, updated_at
         FROM providers
         ORDER BY name ASC",
    )?;
    let rows = stmt.query_map([], read_provider_profile_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
}

fn load_existing_provider_models(
    conn: &Connection,
) -> Result<Vec<StoredProviderModel>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "SELECT provider_name, model_id, display_name, context_limit, output_limit,
                default_temperature, supports_reasoning, created_at, updated_at
         FROM provider_models
         ORDER BY provider_name ASC, created_at DESC, rowid DESC, model_id ASC",
    )?;
    let rows = stmt.query_map([], read_provider_model_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)
}

fn read_provider_profile_row(row: &Row<'_>) -> rusqlite::Result<StoredProviderProfile> {
    Ok(StoredProviderProfile {
        name: row.get(0)?,
        kind: row.get(1)?,
        base_url: row.get(2)?,
        api_key: row.get(3)?,
        is_active: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn read_provider_model_row(row: &Row<'_>) -> rusqlite::Result<StoredProviderModel> {
    Ok(StoredProviderModel {
        provider_name: row.get(0)?,
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

fn parse_provider_kind(kind: &str) -> Result<ProviderKind, AiaStoreError> {
    match kind {
        "openai-responses" => Ok(ProviderKind::OpenAiResponses),
        "openai-chat-completions" => Ok(ProviderKind::OpenAiChatCompletions),
        other => Err(AiaStoreError::new(format!("unknown provider kind: {other}"))),
    }
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

#[cfg(test)]
#[path = "../tests/provider/mod.rs"]
mod tests;
