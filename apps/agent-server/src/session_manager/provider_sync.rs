use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use agent_core::ToolRegistry;
use agent_runtime::AgentRuntime;
use agent_store::AiaStore;
use provider_registry::{CredentialRef, ProviderAccount, ProviderEndpoint, ProviderRegistry};
use session_tape::SessionProviderBinding;

use crate::{
    model::{ServerModel, build_model_from_selection},
    runtime_worker::{
        CreateProviderInput, ProviderInfoSnapshot, RuntimeWorkerError, UpdateProviderInput,
    },
};

use super::{
    SessionId, SessionManagerConfig, SessionSlot, choose_provider_for_tape,
    load_session_tape_with_repair, prepare_runtime_sync, prompt_cache_for_selection,
    refresh_context_stats_snapshot,
};

enum RuntimeSyncMode {
    PreserveSessionBinding,
    RebindTo(SessionProviderBinding),
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
        let mut candidate_registry = self.config.registry.clone();
        candidate_registry.upsert(ProviderAccount {
            id: input.id,
            label: input.label,
            adapter: input.adapter,
            endpoint: ProviderEndpoint { base_url: input.base_url },
            credential: CredentialRef { api_key: input.api_key },
            models: input.models,
        });
        self.sync_registry(candidate_registry).map(|_| ())
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
            .find(|provider| provider.id == name)
            .cloned()
            .ok_or_else(|| RuntimeWorkerError::not_found(format!("provider 不存在：{name}")))?;

        let updated = ProviderAccount {
            id: name.clone(),
            label: input.label.unwrap_or(profile.label),
            adapter: input.adapter.unwrap_or(profile.adapter),
            endpoint: ProviderEndpoint {
                base_url: input.base_url.unwrap_or(profile.endpoint.base_url),
            },
            credential: CredentialRef {
                api_key: input.api_key.unwrap_or(profile.credential.api_key),
            },
            models: input.models.unwrap_or(profile.models),
        };

        let mut candidate_registry = self.config.registry.clone();
        candidate_registry.upsert(updated);
        self.sync_registry(candidate_registry).map(|_| ())
    }

    pub(super) fn delete_provider(&mut self, name: String) -> Result<(), RuntimeWorkerError> {
        let mut candidate_registry = self.config.registry.clone();
        candidate_registry
            .remove(&name)
            .map_err(|error| RuntimeWorkerError::not_found(error.to_string()))?;
        self.sync_registry(candidate_registry).map(|_| ())
    }

    pub(super) fn update_session_provider_binding(
        &mut self,
        session_id: &str,
        binding: SessionProviderBinding,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        if slot.status() == super::SlotStatus::Running {
            return Err(RuntimeWorkerError::bad_request(
                "cannot update session settings while a turn is running",
            ));
        }

        slot.provider_binding = binding.clone();
        let session_path = slot.session_path.clone();
        let context_stats = slot.context_stats.clone();

        match slot.runtime_mut() {
            Some(runtime) => {
                if runtime.tape().latest_provider_binding().as_ref() != Some(&binding) {
                    runtime.tape_mut().bind_provider(binding.clone());
                    runtime.tape().save_jsonl(&session_path).map_err(|error| {
                        RuntimeWorkerError::internal(format!("session save failed: {error}"))
                    })?;
                }
                RuntimeSyncContext::new(
                    session_id,
                    &session_path,
                    &self.config.registry,
                    self.config.store.clone(),
                    RuntimeSyncMode::RebindTo(binding),
                )
                .apply(runtime)?;
                refresh_context_stats_snapshot(&context_stats, runtime);
                return Ok(ProviderInfoSnapshot::from_identity(runtime.model_identity()));
            }
            None => {
                if slot.status() == super::SlotStatus::Running {
                    slot.replace_pending_provider_binding(Some(binding))?;
                    return Ok(match &slot.provider_binding {
                        SessionProviderBinding::Bootstrap => ProviderInfoSnapshot {
                            provider_id: "bootstrap".into(),
                            model_id: "bootstrap".into(),
                            connected: true,
                        },
                        SessionProviderBinding::Provider { model_ref, .. } => {
                            ProviderInfoSnapshot {
                                provider_id: model_ref.provider_id.clone(),
                                model_id: model_ref.model_id.clone(),
                                connected: true,
                            }
                        }
                    });
                }

                let mut tape = load_session_tape_with_repair(&slot.session_path)?;
                if tape.latest_provider_binding().as_ref() != Some(&binding) {
                    tape.bind_provider(binding.clone());
                    tape.save_jsonl(&slot.session_path).map_err(|error| {
                        RuntimeWorkerError::internal(format!("session save failed: {error}"))
                    })?;
                }
                slot.replace_pending_provider_binding(Some(binding))?;
            }
        }

        let tape = load_session_tape_with_repair(&slot.session_path)?;
        let selection = choose_provider_for_tape(&self.config.registry, &tape);
        Ok(ProviderInfoSnapshot::from_identity(&crate::model::model_identity_from_selection(
            &selection,
        )))
    }

    fn sync_registry(
        &mut self,
        candidate_registry: ProviderRegistry,
    ) -> Result<ProviderInfoSnapshot, RuntimeWorkerError> {
        let (info, _, _, _) =
            prepare_runtime_sync(&candidate_registry, Some(self.config.store.clone()))?;

        self.config.store.save_provider_registry(&candidate_registry).map_err(|error| {
            RuntimeWorkerError::internal(format!("provider registry save failed: {error}"))
        })?;

        for (session_id, slot) in self.slots.iter_mut() {
            let session_path = slot.session_path.clone();
            let context_stats = slot.context_stats.clone();
            if let Some(runtime) = slot.runtime_mut() {
                RuntimeSyncContext::new(
                    session_id,
                    &session_path,
                    &candidate_registry,
                    self.config.store.clone(),
                    RuntimeSyncMode::PreserveSessionBinding,
                )
                .apply(runtime)?;
                refresh_context_stats_snapshot(&context_stats, runtime);
            }
        }

        self.config.registry = candidate_registry;
        *super::write_lock(&self.config.provider_registry_snapshot) = self.config.registry.clone();
        *super::write_lock(&self.config.provider_info_snapshot) = info.clone();

        Ok(info)
    }
}

#[cfg(test)]
#[path = "../../tests/session_manager/provider_sync/mod.rs"]
mod tests;
