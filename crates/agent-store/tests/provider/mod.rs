use provider_registry::{ProviderProfile, ProviderRegistry};

#[test]
fn provider_registry_defaults_to_empty_when_missing() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");

    let registry = store.load_provider_registry().expect("registry should load");

    assert!(registry.providers().is_empty());
    assert!(registry.active_provider().is_none());
}

#[test]
fn provider_registry_round_trips_through_sqlite() {
    let store = crate::AiaStore::in_memory().expect("memory store should initialize");
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-4.1-mini",
    ));
    registry.set_active("main").expect("active provider should exist");

    store.save_provider_registry(&registry).expect("registry should save to sqlite");
    let restored = store.load_provider_registry().expect("registry should load from sqlite");

    assert_eq!(restored, registry);
}
