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
fn provider_registry_migrates_legacy_api_key_column_to_credential_columns() {
    let conn = rusqlite::Connection::open_in_memory().expect("memory db should open");
    conn.execute_batch(
        "CREATE TABLE providers (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            adapter    TEXT NOT NULL,
            base_url   TEXT NOT NULL,
            api_key    TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE provider_models (
            provider_id         TEXT NOT NULL,
            model_id            TEXT NOT NULL,
            display_name        TEXT,
            context_limit       INTEGER,
            output_limit        INTEGER,
            default_temperature REAL,
            supports_reasoning  INTEGER NOT NULL,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL,
            PRIMARY KEY (provider_id, model_id)
        );
        INSERT INTO providers (id, label, adapter, base_url, api_key, created_at, updated_at)
        VALUES ('main', 'main', 'openai-responses', 'https://api.openai.com/v1', 'secret', '2026-03-31T00:00:00Z', '2026-03-31T00:00:00Z');
        INSERT INTO provider_models (
            provider_id, model_id, display_name, context_limit, output_limit,
            default_temperature, supports_reasoning, created_at, updated_at
        ) VALUES (
            'main', 'gpt-5-mini', 'GPT-5 Mini', 200000, 32768,
            0.1, 1, '2026-03-31T00:00:00Z', '2026-03-31T00:00:00Z'
        );",
    )
    .expect("legacy schema should initialize");

    let store = crate::AiaStore { conn: std::sync::Mutex::new(conn) };
    store.init_provider_schema().expect("schema init should migrate legacy provider credentials");

    let restored =
        store.load_provider_registry().expect("registry should load from migrated schema");

    assert_eq!(restored.providers().len(), 1);
    assert_eq!(restored.providers()[0].credential, CredentialRef::stored("api_key", "secret"));

    store
        .with_conn(|conn| {
            let (credential_type, credential_value): (String, String) = conn
                .query_row(
                    "SELECT credential_type, credential_value FROM providers WHERE id = 'main'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("credential columns should be readable");
            assert_eq!(credential_type, "api_key");
            assert_eq!(credential_value, "secret");
            Ok::<_, crate::AiaStoreError>(())
        })
        .expect("credential migration should persist values");
}
