use provider_registry::{ModelConfig, ModelLimit, ProviderKind, ProviderProfile, ProviderRegistry};

#[test]
fn provider_registry_defaults_to_empty_when_missing() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");

    let registry = store.load_provider_registry().expect("registry should load");

    assert!(registry.providers().is_empty());
    assert!(registry.active_provider().is_none());
}

#[test]
fn provider_registry_round_trips_through_normalized_tables() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile {
        name: "main".into(),
        kind: ProviderKind::OpenAiResponses,
        base_url: "https://api.openai.com/v1".into(),
        api_key: "secret".into(),
        models: vec![
            ModelConfig {
                id: "gpt-4.1-mini".into(),
                display_name: Some("GPT-4.1 Mini".into()),
                limit: Some(ModelLimit { context: Some(200_000), output: Some(16_384) }),
                default_temperature: Some(0.2),
                supports_reasoning: true,
            },
            ModelConfig {
                id: "gpt-4.1".into(),
                display_name: Some("GPT-4.1".into()),
                limit: Some(ModelLimit { context: Some(200_000), output: Some(32_768) }),
                default_temperature: Some(0.1),
                supports_reasoning: true,
            },
        ],
        active_model: Some("gpt-4.1".into()),
    });
    registry.set_active("main").expect("active provider should exist");

    store.save_provider_registry(&registry).expect("registry should save to sqlite");
    let restored = store.load_provider_registry().expect("registry should load from sqlite");

    assert_eq!(restored.providers().len(), 1);
    assert_eq!(restored.active_provider().map(|provider| provider.name.as_str()), Some("main"));
    assert_eq!(restored.providers()[0].models.len(), 2);
    assert_eq!(restored.providers()[0].active_model_id(), Some("gpt-4.1"));
    assert!(restored.providers()[0].has_model("gpt-4.1-mini"));
    assert!(restored.providers()[0].has_model("gpt-4.1"));
}
