use std::{fs, path::Path};

use aia_config::PROVIDERS_FILE_NAME;
use serde::{Deserialize, Serialize};

use crate::{ProviderProfile, error::ProviderRegistryError};

pub fn default_registry_path() -> std::path::PathBuf {
    aia_config::default_registry_path()
}

fn legacy_registry_path(path: &Path) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    Some(parent.join("sessions").join(PROVIDERS_FILE_NAME))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: Vec<ProviderProfile>,
    active_provider: Option<String>,
}

impl ProviderRegistry {
    pub fn load_or_default(path: &Path) -> Result<Self, ProviderRegistryError> {
        if !path.exists() {
            if let Some(legacy_path) = legacy_registry_path(path)
                && legacy_path.exists()
            {
                let contents = fs::read_to_string(&legacy_path)
                    .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
                return serde_json::from_str(&contents)
                    .map_err(|error| ProviderRegistryError::new(error.to_string()));
            }
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
        serde_json::from_str(&contents)
            .map_err(|error| ProviderRegistryError::new(error.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), ProviderRegistryError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
        }

        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| ProviderRegistryError::new(error.to_string()))?;
        fs::write(path, contents).map_err(|error| ProviderRegistryError::new(error.to_string()))
    }

    pub fn upsert(&mut self, mut provider: ProviderProfile) {
        provider.normalize_active_model();
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
