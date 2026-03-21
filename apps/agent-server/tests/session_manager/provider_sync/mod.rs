use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::RequestTimeoutConfig;
use provider_registry::{ProviderProfile, ProviderRegistry};

use super::{ProviderSyncService, ReturnedRuntimeSync};
use crate::runtime_worker::{SwitchProviderInput, UpdateProviderInput};
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
        provider_registry_path: root.join("providers.json"),
        broadcast_tx: tokio::sync::broadcast::channel(8).0,
        provider_registry_snapshot: Arc::new(RwLock::new(registry)),
        provider_info_snapshot: Arc::new(RwLock::new(provider_info)),
        workspace_root: root.to_path_buf(),
        user_agent: "test-agent".into(),
        request_timeout: RequestTimeoutConfig {
            read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
        },
        system_prompt: agent_prompts::SystemPromptConfig::default(),
        runtime_hooks: agent_runtime::RuntimeHooks::default(),
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
            .runtime
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
                active_model: None,
                api_key: Some("backup-key-updated".into()),
                base_url: None,
            },
        )
        .expect("provider update should succeed");

    assert_eq!(
        slots["session-1"]
            .runtime
            .as_ref()
            .expect("runtime should exist")
            .tape()
            .latest_provider_binding(),
        None,
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn switching_provider_marks_running_session_for_return_sync() {
    let root = temp_root("switch-running");
    let mut config = sample_config(&root, sample_registry());
    let mut slot =
        SessionSlotFactory::new(&config).create("session-1").expect("session slot should build");
    let mut runtime = slot.runtime.take().expect("runtime should exist");
    slot.status = super::super::SlotStatus::Running;

    let mut slots = HashMap::new();
    slots.insert("session-1".to_string(), slot);

    let mut service = ProviderSyncService::new(&mut slots, &mut config);
    service
        .switch_provider(SwitchProviderInput { name: "backup".into(), model_id: None })
        .expect("provider switch should succeed");

    let pending_binding = slots["session-1"].pending_provider_binding.clone();
    let session_path = slots["session-1"].session_path.clone();
    assert!(matches!(
        pending_binding,
        Some(SessionProviderBinding::Provider { ref name, .. }) if name == "backup"
    ));

    ReturnedRuntimeSync::new(
        "session-1",
        &session_path,
        &config.registry,
        config.store.clone(),
        pending_binding.clone(),
    )
    .apply(&mut runtime)
    .expect("returned runtime should resync");

    assert_eq!(runtime.tape().latest_provider_binding(), pending_binding);
    assert_eq!(runtime.model_identity().name, "model-backup");

    let _ = std::fs::remove_dir_all(root);
}
