use serde::{Deserialize, Serialize};

use crate::{ProviderProfile, error::ProviderRegistryError};

pub fn default_registry_path() -> std::path::PathBuf {
    aia_config::default_registry_path()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: Vec<ProviderProfile>,
    active_provider: Option<String>,
}

impl ProviderRegistry {
    pub fn upsert(&mut self, provider: ProviderProfile) {
        if let Some(existing) =
            self.providers.iter_mut().find(|existing| existing.name == provider.name)
        {
            *existing = provider;
            return;
        }

        if self.active_provider.is_none() {
            self.active_provider = Some(provider.name.clone());
        }

        self.providers.push(provider);
    }

    pub fn remove(&mut self, provider_name: &str) -> Result<(), ProviderRegistryError> {
        let Some(index) = self.providers.iter().position(|provider| provider.name == provider_name)
        else {
            return Err(ProviderRegistryError::new(format!("provider 不存在：{provider_name}")));
        };

        self.providers.remove(index);
        if self.active_provider.as_deref() == Some(provider_name) {
            self.active_provider = self.providers.first().map(|provider| provider.name.clone());
        }
        Ok(())
    }

    pub fn set_active(&mut self, provider_name: &str) -> Result<(), ProviderRegistryError> {
        if !self.providers.iter().any(|provider| provider.name == provider_name) {
            return Err(ProviderRegistryError::new(format!("provider 不存在：{provider_name}")));
        }

        self.active_provider = Some(provider_name.to_string());
        Ok(())
    }

    pub fn active_provider(&self) -> Option<&ProviderProfile> {
        let active_name = self.active_provider.as_ref()?;
        self.providers.iter().find(|provider| provider.name == *active_name)
    }

    pub fn providers(&self) -> &[ProviderProfile] {
        &self.providers
    }
}
