use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use agent_core::ToolRegistry;
use agent_runtime::AgentRuntime;
use agent_store::AiaStore;
use provider_registry::ProviderRegistry;
use session_tape::{SessionProviderBinding, SessionTape};

use crate::{
    model::{ProviderLaunchChoice, ServerModel, build_model_from_selection},
    runtime_worker::{
        CreateProviderInput, ProviderInfoSnapshot, RuntimeWorkerError, SwitchProviderInput,
        UpdateProviderInput,
    },
};

use super::{
    SessionId, SessionManagerConfig, SessionSlot, choose_provider_for_tape, prepare_runtime_sync,
    prompt_cache_for_selection, refresh_context_stats_snapshot,
};

enum RuntimeSyncMode {
    PreserveSessionBinding,
    RebindTo(SessionProviderBinding),
}

enum RegistrySyncPolicy {
    PreserveSessionBindings,
    SwitchActiveProvider {
        previous_registry: ProviderRegistry,
        next_active_binding: SessionProviderBinding,
    },
}

struct RuntimeSyncContext<'a> {
    session_id: &'a str,
    session_path: &'a Path,
    registry: &'a ProviderRegistry,
    store: Arc<AiaStore>,
    mode: RuntimeSyncMode,
}

impl<'a> RuntimeSyncContext<'a> {
    fn new(
        session_id: &'a str,
        session_path: &'a Path,
        registry: &'a ProviderRegistry,
        store: Arc<AiaStore>,
        mode: RuntimeSyncMode,
    ) -> Self {
        Self { session_id, session_path, registry, store, mode }
    }

    fn apply(
        &self,
        runtime: &mut AgentRuntime<ServerModel, ToolRegistry>,
    ) -> Result<(), RuntimeWorkerError> {
        let selection = match &self.mode {
            RuntimeSyncMode::PreserveSessionBinding => {
                choose_provider_for_tape(self.registry, runtime.tape())
            }
            RuntimeSyncMode::RebindTo(binding) => {
                if runtime.tape().latest_provider_binding().as_ref() != Some(binding) {
                    runtime.tape_mut().bind_provider(binding.clone());
                    runtime.tape().save_jsonl(self.session_path).map_err(|error| {
                        RuntimeWorkerError::internal(format!("session save failed: {error}"))
                    })?;
                }
                choose_provider_for_tape(self.registry, runtime.tape())
            }
        };

        let (identity, new_model) =
            build_model_from_selection(selection.clone(), Some(self.store.clone())).map_err(
                |error: crate::model::ServerSetupError| {
                    RuntimeWorkerError::internal(error.to_string())
                },
            )?;
        runtime.replace_model(new_model, identity);
        runtime.set_prompt_cache(prompt_cache_for_selection(&selection, self.session_id));
        Ok(())
    }
}

pub(super) struct ReturnedRuntimeSync<'a> {
    session_id: &'a str,
    session_path: &'a Path,
    registry: &'a ProviderRegistry,
    store: Arc<AiaStore>,
    pending_binding: Option<SessionProviderBinding>,
}

impl<'a> ReturnedRuntimeSync<'a> {
    pub(super) fn new(
        session_id: &'a str,
        session_path: &'a Path,
        registry: &'a ProviderRegistry,
        store: Arc<AiaStore>,
        pending_binding: Option<SessionProviderBinding>,
    ) -> Self {
        Self { session_id, session_path, registry, store, pending_binding }
    }

    pub(super) fn apply(
        self,
        runtime: &mut AgentRuntime<ServerModel, ToolRegistry>,
    ) -> Result<(), RuntimeWorkerError> {
        let mode = match self.pending_binding {
            Some(binding) => RuntimeSyncMode::RebindTo(binding),
            None => RuntimeSyncMode::PreserveSessionBinding,
        };
        RuntimeSyncContext::new(self.session_id, self.session_path, self.registry, self.store, mode)
            .apply(runtime)
    }
}

pub(super) struct ProviderSyncService<'a> {
    slots: &'a mut HashMap<SessionId, SessionSlot>,
    config: &'a mut SessionManagerConfig,
}

impl<'a> ProviderSyncService<'a> {
    pub(super) fn new(
        slots: &'a mut HashMap<SessionId, SessionSlot>,
        config: &'a mut SessionManagerConfig,
    ) -> Self {
        Self { slots, config }
    }

    pub(super) fn create_provider(
        &mut self,
        input: CreateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        let active_model =
            input.active_model.or_else(|| input.models.first().map(|m| m.id.clone()));
        let mut candidate_registry = self.config.registry.clone();
        candidate_registry.upsert(provider_registry::ProviderProfile {
            name: input.name,
            kind: input.kind,
            base_url: input.base_url,
            api_key: input.api_key,
            models: input.models,
            active_model,
        });
        self.sync_registry(candidate_registry, RegistrySyncPolicy::PreserveSessionBindings)
            .map(|_| ())
    }

    pub(super) fn update_provider(
        &mut self,
        name: String,
        input: UpdateProviderInput,
    ) -> Result<(), RuntimeWorkerError> {
        let profile = self
            .config
            .registry
            .providers()
            .iter()
            .find(|provider| provider.name == name)
            .cloned()
            .ok_or_else(|| RuntimeWorkerError::not_found(format!("provider 不存在：{name}")))?;

        let updated = provider_registry::ProviderProfile {
            name: name.clone(),
            kind: input.kind.unwrap_or(profile.kind),
            base_url: input.base_url.unwrap_or(profile.base_url),
            api_key: input.api_key.unwrap_or(profile.api_key),
            models: input.models.unwrap_or(profile.models),
            active_model: input.active_model.or(profile.active_model),
        };

        let mut candidate_registry = self.config.registry.clone();
        candidate_registry.upsert(updated);
        self.sync_registry(candidate_registry, RegistrySyncPolicy::PreserveSessionBindings)
            .map(|_| ())
    }

    pub(super) fn delete_provider(&mut self, name: String) -> Result<(), RuntimeWorkerError> {
        let mut candidate_registry = self.config.registry.clone();
        candidate_registry
            .remove(&name)
            .map_err(|error| RuntimeWorkerError::not_found(error.to_string()))?;
        self.sync_registry(candidate_registry, RegistrySyncPolicy::PreserveSessionBindings)
            .map(|_| ())
    }

    pub(super) fn switch_provider(
        &mut self,
        input: SwitchProviderInput,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        let mut profile = self
            .config
            .registry
            .providers()
            .iter()
            .find(|provider| provider.name == input.name)
            .cloned()
            .ok_or_else(|| {
                RuntimeWorkerError::not_found(format!("provider 不存在：{}", input.name))
            })?;

        if let Some(model_id) = &input.model_id {
            if !profile.has_model(model_id) {
                return Err(RuntimeWorkerError::bad_request(format!("模型不存在：{model_id}")));
            }
            profile.active_model = Some(model_id.to_string());
        }

        let previous_registry = self.config.registry.clone();
        let mut candidate_registry = previous_registry.clone();
        candidate_registry.upsert(profile);
        candidate_registry
            .set_active(&input.name)
            .map_err(|error| RuntimeWorkerError::bad_request(error.to_string()))?;

        let next_active_binding =
            prepare_runtime_sync(&candidate_registry, Some(self.config.store.clone()))?.3;
        self.sync_registry(
            candidate_registry,
            RegistrySyncPolicy::SwitchActiveProvider { previous_registry, next_active_binding },
        )
    }

    fn sync_registry(
        &mut self,
        candidate_registry: ProviderRegistry,
        policy: RegistrySyncPolicy,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        let (info, _, _, _) =
            prepare_runtime_sync(&candidate_registry, Some(self.config.store.clone()))?;

        candidate_registry.save(&self.config.provider_registry_path).map_err(|error| {
            RuntimeWorkerError::internal(format!("provider registry save failed: {error}"))
        })?;

        for (session_id, slot) in self.slots.iter_mut() {
            match (&mut slot.runtime, &policy) {
                (Some(runtime), RegistrySyncPolicy::PreserveSessionBindings) => {
                    RuntimeSyncContext::new(
                        session_id,
                        &slot.session_path,
                        &candidate_registry,
                        self.config.store.clone(),
                        RuntimeSyncMode::PreserveSessionBinding,
                    )
                    .apply(runtime)?;
                    refresh_context_stats_snapshot(&slot.context_stats, runtime);
                    slot.pending_provider_binding = None;
                }
                (
                    Some(runtime),
                    RegistrySyncPolicy::SwitchActiveProvider {
                        previous_registry,
                        next_active_binding,
                    },
                ) => {
                    let mode = if tape_follows_active_provider(runtime.tape(), previous_registry) {
                        RuntimeSyncMode::RebindTo(next_active_binding.clone())
                    } else {
                        RuntimeSyncMode::PreserveSessionBinding
                    };
                    RuntimeSyncContext::new(
                        session_id,
                        &slot.session_path,
                        &candidate_registry,
                        self.config.store.clone(),
                        mode,
                    )
                    .apply(runtime)?;
                    refresh_context_stats_snapshot(&slot.context_stats, runtime);
                    slot.pending_provider_binding = None;
                }
                (
                    None,
                    RegistrySyncPolicy::SwitchActiveProvider {
                        previous_registry,
                        next_active_binding,
                    },
                ) => {
                    let tape = SessionTape::load_jsonl_or_default(&slot.session_path).map_err(
                        |error| RuntimeWorkerError::internal(format!("tape load failed: {error}")),
                    )?;
                    slot.pending_provider_binding =
                        if tape_follows_active_provider(&tape, previous_registry) {
                            Some(next_active_binding.clone())
                        } else {
                            None
                        };
                }
                (None, RegistrySyncPolicy::PreserveSessionBindings) => {}
            }
        }

        self.config.registry = candidate_registry;
        *super::write_lock(&self.config.provider_registry_snapshot) = self.config.registry.clone();
        *super::write_lock(&self.config.provider_info_snapshot) = info.clone();

        Ok(info)
    }
}

fn tape_follows_active_provider(tape: &SessionTape, registry: &ProviderRegistry) -> bool {
    match (choose_provider_for_tape(registry, tape), registry.active_provider()) {
        (ProviderLaunchChoice::Bootstrap, None) => true,
        (ProviderLaunchChoice::OpenAi(profile), Some(active)) => profile.name == active.name,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let provider_info = prepare_runtime_sync(&registry, Some(store.clone()))
            .expect("provider info should build")
            .0;

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
        }
    }

    #[test]
    fn updating_non_active_provider_keeps_unbound_session_unmodified() {
        let root = temp_root("update-non-active");
        let mut config = sample_config(&root, sample_registry());
        let mut slots = HashMap::new();
        let slot = SessionSlotFactory::new(&config)
            .create("session-1")
            .expect("session slot should build");
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
        let mut slot = SessionSlotFactory::new(&config)
            .create("session-1")
            .expect("session slot should build");
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
}
