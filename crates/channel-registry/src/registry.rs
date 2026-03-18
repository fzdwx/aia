use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{ChannelProfile, ChannelRegistryError};

pub fn default_registry_path() -> std::path::PathBuf {
    aia_config::default_channels_path()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRegistry {
    channels: Vec<ChannelProfile>,
}

impl ChannelRegistry {
    pub fn load_or_default(path: &Path) -> Result<Self, ChannelRegistryError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .map_err(|error| ChannelRegistryError::new(error.to_string()))?;
        serde_json::from_str(&contents)
            .map_err(|error| ChannelRegistryError::new(error.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), ChannelRegistryError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ChannelRegistryError::new(error.to_string()))?;
        }
        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| ChannelRegistryError::new(error.to_string()))?;
        fs::write(path, contents).map_err(|error| ChannelRegistryError::new(error.to_string()))
    }

    pub fn upsert(&mut self, profile: ChannelProfile) {
        if let Some(existing) = self.channels.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
            return;
        }
        self.channels.push(profile);
    }

    pub fn remove(&mut self, channel_id: &str) -> Result<(), ChannelRegistryError> {
        let Some(index) = self.channels.iter().position(|item| item.id == channel_id) else {
            return Err(ChannelRegistryError::new(format!("channel 不存在：{channel_id}")));
        };
        self.channels.remove(index);
        Ok(())
    }

    pub fn channels(&self) -> &[ChannelProfile] {
        &self.channels
    }

    pub fn get(&self, channel_id: &str) -> Option<&ChannelProfile> {
        self.channels.iter().find(|item| item.id == channel_id)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aia_config::CHANNELS_FILE_NAME;

    use super::*;

    #[test]
    fn default_path_points_to_channels_json() {
        assert_eq!(default_registry_path(), PathBuf::from(format!(".aia/{CHANNELS_FILE_NAME}")));
    }
}
