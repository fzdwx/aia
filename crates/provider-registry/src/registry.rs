use serde::{Deserialize, Serialize};

use agent_core::ModelRef;

use crate::{ProviderAccount, ResolvedModelSpec, error::ProviderRegistryError};

pub fn default_registry_path() -> std::path::PathBuf {
    aia_config::default_registry_path()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: Vec<ProviderAccount>,
}

impl ProviderRegistry {
    pub fn upsert(&mut self, provider: ProviderAccount) {
        if let Some(existing) =
            self.providers.iter_mut().find(|existing| existing.id == provider.id)
        {
            *existing = provider;
            return;
        }

        self.providers.push(provider);
    }

    pub fn remove(&mut self, provider_id: &str) -> Result<(), ProviderRegistryError> {
        let Some(index) = self.providers.iter().position(|provider| provider.id == provider_id)
        else {
            return Err(ProviderRegistryError::new(format!("provider 不存在：{provider_id}")));
        };

        self.providers.remove(index);
        Ok(())
    }

    /// Returns the first provider, if any exists.
    /// New sessions will use the latest session's provider binding if available,
    /// otherwise fall back to this.
    pub fn first_provider(&self) -> Option<&ProviderAccount> {
        self.providers.first()
    }

    pub fn providers(&self) -> &[ProviderAccount] {
        &self.providers
    }

    pub fn provider(&self, provider_id: &str) -> Option<&ProviderAccount> {
        self.providers.iter().find(|provider| provider.id == provider_id)
    }

    pub fn resolve_model(
        &self,
        model_ref: &ModelRef,
    ) -> Result<ResolvedModelSpec, ProviderRegistryError> {
        let provider = self.provider(&model_ref.provider_id).ok_or_else(|| {
            ProviderRegistryError::new(format!("provider 不存在：{}", model_ref.provider_id))
        })?;
        let model = provider
            .models
            .iter()
            .find(|candidate| candidate.id == model_ref.model_id)
            .cloned()
            .ok_or_else(|| {
                ProviderRegistryError::new(format!(
                    "模型不存在：{}/{}",
                    model_ref.provider_id, model_ref.model_id
                ))
            })?;

        Ok(ResolvedModelSpec {
            model_ref: model_ref.clone(),
            adapter: provider.adapter.clone(),
            base_url: provider.endpoint.base_url.clone(),
            credential: provider.credential.clone(),
            model,
        })
    }

    pub fn first_model_ref(&self) -> Option<ModelRef> {
        let provider = self.first_provider()?;
        let model = provider.default_model_id()?;
        Some(ModelRef::new(provider.id.clone(), model.to_string()))
    }
}
