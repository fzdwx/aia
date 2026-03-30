use serde::{Deserialize, Serialize};

use crate::{ProviderProfile, error::ProviderRegistryError};

pub fn default_registry_path() -> std::path::PathBuf {
    aia_config::default_registry_path()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: Vec<ProviderProfile>,
}

impl ProviderRegistry {
    pub fn upsert(&mut self, provider: ProviderProfile) {
        if let Some(existing) =
            self.providers.iter_mut().find(|existing| existing.name == provider.name)
        {
            *existing = provider;
            return;
        }

        self.providers.push(provider);
    }

    pub fn remove(&mut self, provider_name: &str) -> Result<(), ProviderRegistryError> {
        let Some(index) = self.providers.iter().position(|provider| provider.name == provider_name)
        else {
            return Err(ProviderRegistryError::new(format!("provider 不存在：{provider_name}")));
        };

        self.providers.remove(index);
        Ok(())
    }

    /// Returns the first provider, if any exists.
    /// New sessions will use the latest session's provider binding if available,
    /// otherwise fall back to this.
    pub fn first_provider(&self) -> Option<&ProviderProfile> {
        self.providers.first()
    }

    pub fn providers(&self) -> &[ProviderProfile] {
        &self.providers
    }
}
