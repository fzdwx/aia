use provider_registry::{
    AdapterKind, CredentialRef, ModelConfig, ModelLimit, ProviderAccount, ProviderEndpoint,
    ProviderRegistry,
};

#[test]
fn provider_registry_defaults_to_empty_when_missing() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");

    let registry = store.load_provider_registry().expect("registry should load");

    assert!(registry.providers().is_empty());
    assert!(registry.first_provider().is_none());
}

#[test]
fn provider_registry_round_trips_through_normalized_tables() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount {
        id: "main".into(),
        label: "main".into(),
        adapter: AdapterKind::OpenAiResponses,
        endpoint: ProviderEndpoint { base_url: "https://api.openai.com/v1".into() },
        credential: CredentialRef::api_key("secret"),
        models: vec![
            ModelConfig {
                id: "gpt-4.1".into(),
                display_name: Some("GPT-4.1".into()),
                limit: Some(ModelLimit { context: Some(200_000), output: Some(32_768) }),
                default_temperature: Some(0.1),
                supports_reasoning: true,
            },
            ModelConfig {
                id: "gpt-4.1-mini".into(),
                display_name: Some("GPT-4.1 Mini".into()),
                limit: Some(ModelLimit { context: Some(200_000), output: Some(16_384) }),
                default_temperature: Some(0.2),
                supports_reasoning: true,
            },
        ],
    });

    store.save_provider_registry(&registry).expect("registry should save to sqlite");
    let restored = store.load_provider_registry().expect("registry should load from sqlite");

    assert_eq!(restored.providers().len(), 1);
    assert_eq!(restored.first_provider().map(|provider| provider.id.as_str()), Some("main"));
    assert_eq!(restored.providers()[0].models.len(), 2);
    assert_eq!(restored.providers()[0].default_model_id(), Some("gpt-4.1"));
    assert!(restored.providers()[0].has_model("gpt-4.1-mini"));
    assert!(restored.providers()[0].has_model("gpt-4.1"));
}

#[test]
fn provider_registry_persists_providers_and_newest_model_order() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount {
        id: "main".into(),
        label: "main".into(),
        adapter: AdapterKind::OpenAiResponses,
        endpoint: ProviderEndpoint { base_url: "https://api.openai.com/v1".into() },
        credential: CredentialRef::api_key("secret"),
        models: vec![ModelConfig::new("gpt-4.1"), ModelConfig::new("gpt-4.1-mini")],
    });
    registry.upsert(ProviderAccount::openai_chat_completions(
        "backup",
        "https://example.com/v1",
        "secret-2",
        "glm-4.6",
    ));
    store.save_provider_registry(&registry).expect("registry should save to sqlite");

    let mut updated = store.load_provider_registry().expect("registry should load from sqlite");
    let main = updated
        .providers()
        .iter()
        .find(|provider| provider.id == "main")
        .expect("main provider should exist");
    assert_eq!(main.default_model_id(), Some("gpt-4.1"));

    updated.upsert(ProviderAccount {
        id: "main".into(),
        label: "main".into(),
        adapter: AdapterKind::OpenAiResponses,
        endpoint: ProviderEndpoint { base_url: "https://api.openai.com/v1".into() },
        credential: CredentialRef::api_key("secret"),
        models: vec![
            ModelConfig::new("gpt-5"),
            ModelConfig::new("gpt-4.1"),
            ModelConfig::new("gpt-4.1-mini"),
        ],
    });
    store.save_provider_registry(&updated).expect("updated registry should save");

    let restored = store.load_provider_registry().expect("updated registry should reload");
    let main = restored
        .providers()
        .iter()
        .find(|provider| provider.id == "main")
        .expect("main provider should exist");
    assert_eq!(restored.first_provider().map(|provider| provider.id.as_str()), Some("backup"));
    assert_eq!(
        main.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
        vec!["gpt-5", "gpt-4.1", "gpt-4.1-mini"]
    );
    assert_eq!(main.default_model_id(), Some("gpt-5"));
}

#[test]
fn legacy_provider_tables_are_migrated_to_current_schema() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");

    store
        .with_conn(|conn| {
            conn.execute_batch(
                "DROP TABLE provider_models;
                 DROP TABLE providers;
                 CREATE TABLE providers (
                     name         TEXT PRIMARY KEY,
                     kind         TEXT NOT NULL,
                     base_url     TEXT NOT NULL,
                     api_key      TEXT NOT NULL,
                     created_at   TEXT NOT NULL,
                     updated_at   TEXT NOT NULL,
                     is_active    INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE provider_models (
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
                 CREATE INDEX idx_provider_models_provider_name
                     ON provider_models(provider_name);
                 INSERT INTO providers (name, kind, base_url, api_key, created_at, updated_at, is_active)
                 VALUES ('legacy-main', 'openai-responses', 'https://api.openai.com/v1', 'secret', '2026-03-01T00:00:00Z', '2026-03-02T00:00:00Z', 1);
                 INSERT INTO provider_models (
                     provider_name, model_id, display_name, context_limit, output_limit,
                     default_temperature, supports_reasoning, created_at, updated_at
                 )
                 VALUES (
                     'legacy-main', 'gpt-5', 'GPT-5', 200000, 8192,
                     0.2, 1, '2026-03-01T00:00:00Z', '2026-03-02T00:00:00Z'
                 );",
            )?;
            Ok(())
        })
        .expect("legacy schema should be installable");

    store.init_provider_schema().expect("legacy provider schema should migrate on startup");

    let registry = store.load_provider_registry().expect("migrated provider registry should load");

    assert_eq!(registry.providers().len(), 1);
    let provider = &registry.providers()[0];
    assert_eq!(provider.id, "legacy-main");
    assert_eq!(provider.label, "legacy-main");
    assert_eq!(provider.adapter, AdapterKind::OpenAiResponses);
    assert_eq!(provider.endpoint.base_url, "https://api.openai.com/v1");
    assert_eq!(provider.credential.api_key_value(), "secret");
    assert_eq!(provider.default_model_id(), Some("gpt-5"));

    store
        .with_conn(|conn| {
            let provider_id_exists = {
                let mut stmt = conn.prepare("PRAGMA table_info(provider_models)")?;
                let columns =
                    stmt.query_map([], |row| row.get::<_, String>(1))?
                        .collect::<Result<Vec<_>, _>>()?;
                columns.into_iter().any(|column| column == "provider_id")
            };
            let provider_name_exists = {
                let mut stmt = conn.prepare("PRAGMA table_info(provider_models)")?;
                let columns =
                    stmt.query_map([], |row| row.get::<_, String>(1))?
                        .collect::<Result<Vec<_>, _>>()?;
                columns.into_iter().any(|column| column == "provider_name")
            };

            assert!(provider_id_exists);
            assert!(!provider_name_exists);
            Ok(())
        })
        .expect("migrated schema should expose current columns");
}
