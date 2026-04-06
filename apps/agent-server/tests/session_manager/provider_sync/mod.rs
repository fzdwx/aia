use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::RequestTimeoutConfig;
use provider_registry::{ProviderProfile, ProviderRegistry};

use super::ProviderSyncService;
use crate::runtime_worker::UpdateProviderInput;
use crate::session_manager::{SessionManagerConfig, SessionSlotFactory, prepare_runtime_sync};
use session_tape::SessionProviderBinding;

fn temp_root(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-provider-sync-{name}-{suffix}"))
}

fn sample_registry() -> ProviderRegistry {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile::openai_responses(
        "primary",
        "https://primary.example.com",
        "primary-key",
        "model-primary",
    ));
    registry.upsert(ProviderProfile::openai_responses(
        "backup",
        "https://backup.example.com",
        "backup-key",
        "model-backup",
    ));
    registry
}

fn sample_config(root: &std::path::Path, registry: ProviderRegistry) -> SessionManagerConfig {
    std::fs::create_dir_all(root.join("sessions")).expect("sessions dir should exist");
    let store = Arc::new(
        agent_store::AiaStore::new(root.join("store.sqlite3"))
            .expect("sqlite store should initialize"),
    );
    let provider_info =
        prepare_runtime_sync(&registry, Some(store.clone())).expect("provider info should build").0;

    SessionManagerConfig {
        sessions_dir: root.join("sessions"),
        store,
        registry: registry.clone(),
        broadcast_tx: tokio::sync::broadcast::channel(8).0,
        provider_registry_snapshot: Arc::new(RwLock::new(registry)),
        provider_info_snapshot: Arc::new(RwLock::new(provider_info)),
        workspace_root: root.to_path_buf(),
        user_agent: "test-agent".into(),
        request_timeout: RequestTimeoutConfig {
            read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
        },
        system_prompt: None,
        runtime_hooks: agent_runtime::RuntimeHooks::default(),
        runtime_tool_host: Arc::new(crate::session_manager::RuntimeToolHost {
            tx: tokio::sync::mpsc::channel(8).0,
        }),
    }
}

#[test]
fn updating_non_active_provider_keeps_unbound_session_unmodified() {
    let root = temp_root("update-non-active");
    let mut config = sample_config(&root, sample_registry());
    let mut slots = HashMap::new();
    let slot =
        SessionSlotFactory::new(&config).create("session-1").expect("session slot should build");
    slots.insert("session-1".to_string(), slot);

    assert_eq!(
        slots["session-1"]
            .runtime()
            .as_ref()
            .expect("runtime should exist")
            .tape()
            .latest_provider_binding(),
        None,
    );

    let mut service = ProviderSyncService::new(&mut slots, &mut config);
    service
        .update_provider(
            "backup".into(),
            UpdateProviderInput {
                kind: None,
                models: None,
                api_key: Some("backup-key-updated".into()),
                base_url: None,
            },
        )
        .expect("provider update should succeed");

    assert_eq!(
        slots["session-1"]
            .runtime()
            .as_ref()
            .expect("runtime should exist")
            .tape()
            .latest_provider_binding(),
        None,
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn session_binding_preserves_reasoning_effort_override() {
    let root = temp_root("session-binding-reasoning");
    let mut config = sample_config(&root, sample_registry());
    let mut slots = HashMap::new();
    let slot =
        SessionSlotFactory::new(&config).create("session-1").expect("session slot should build");
    slots.insert("session-1".to_string(), slot);

    let mut service = ProviderSyncService::new(&mut slots, &mut config);
    service
        .update_session_provider_binding(
            "session-1",
            SessionProviderBinding::Provider {
                name: "primary".into(),
                model: "model-primary".into(),
                base_url: "https://primary.example.com".into(),
                protocol: "openai-responses".into(),
                reasoning_effort: Some("high".into()),
            },
        )
        .expect("session settings update should succeed");

    let binding = slots["session-1"]
        .runtime()
        .as_ref()
        .expect("runtime should exist")
        .tape()
        .latest_provider_binding();
    assert!(matches!(
        binding,
        Some(SessionProviderBinding::Provider {
            reasoning_effort: Some(ref effort),
            ..
        }) if effort == "high"
    ));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn running_session_settings_update_is_rejected() {
    let root = temp_root("session-binding-running-rejected");
    let mut config = sample_config(&root, sample_registry());
    let mut slots = HashMap::new();
    let mut slot =
        SessionSlotFactory::new(&config).create("session-1").expect("session slot should build");
    let session_path = slot.session_path.clone();
    let original_contents = std::fs::read_to_string(&session_path).unwrap_or_default();

    let (_runtime, _subscriber, _running_turn) =
        slot.begin_turn().expect("idle slot should start turn");
    slots.insert("session-1".to_string(), slot);

    let mut service = ProviderSyncService::new(&mut slots, &mut config);
    let binding = SessionProviderBinding::Provider {
        name: "primary".into(),
        model: "model-primary".into(),
        base_url: "https://primary.example.com".into(),
        protocol: "openai-responses".into(),
        reasoning_effort: Some("high".into()),
    };

    let error = service
        .update_session_provider_binding("session-1", binding)
        .expect_err("running session settings update should be rejected");

    assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
    assert!(error.message.contains("cannot update session settings while a turn is running"));
    assert_eq!(std::fs::read_to_string(&session_path).unwrap_or_default(), original_contents);

    let _ = std::fs::remove_dir_all(root);
}
