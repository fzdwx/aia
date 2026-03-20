use std::sync::Arc;

use agent_store::{AiaStore, StoredChannelProfile};

use crate::{ChannelBridgeError, ChannelProfile};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelProfileRegistry {
    channels: Vec<ChannelProfile>,
}

impl ChannelProfileRegistry {
    pub async fn load_from_store(store: &Arc<AiaStore>) -> Result<Self, ChannelBridgeError> {
        let rows = store
            .list_channel_profiles_async()
            .await
            .map_err(|error| ChannelBridgeError::new(error.to_string()))?;
        let mut channels = Vec::with_capacity(rows.len());
        for row in rows {
            channels.push(read_profile_row(row)?);
        }
        Ok(Self { channels })
    }

    pub async fn upsert_into_store(
        store: &Arc<AiaStore>,
        profile: ChannelProfile,
    ) -> Result<(), ChannelBridgeError> {
        let config_json = serde_json::to_string(&profile.config)
            .map_err(|error| ChannelBridgeError::new(error.to_string()))?;
        store
            .upsert_channel_profile_async(StoredChannelProfile::new(
                profile.id,
                profile.name,
                serde_json::to_string(&profile.transport)
                    .map_err(|error| ChannelBridgeError::new(error.to_string()))?,
                profile.enabled,
                config_json,
            ))
            .await
            .map_err(|error| ChannelBridgeError::new(error.to_string()))
    }

    pub async fn delete_from_store(
        store: &Arc<AiaStore>,
        channel_id: &str,
    ) -> Result<(), ChannelBridgeError> {
        store
            .delete_channel_profile_async(channel_id.to_string())
            .await
            .map_err(|error| ChannelBridgeError::new(error.to_string()))?;
        Ok(())
    }

    pub fn upsert(&mut self, profile: ChannelProfile) {
        if let Some(existing) = self.channels.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
            return;
        }
        self.channels.push(profile);
    }

    pub fn remove(&mut self, channel_id: &str) -> Result<(), ChannelBridgeError> {
        let Some(index) = self.channels.iter().position(|item| item.id == channel_id) else {
            return Err(ChannelBridgeError::new(format!("channel 不存在：{channel_id}")));
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

fn read_profile_row(row: StoredChannelProfile) -> Result<ChannelProfile, ChannelBridgeError> {
    let transport = serde_json::from_str(&row.transport)
        .map_err(|error| ChannelBridgeError::new(error.to_string()))?;
    let config = serde_json::from_str(&row.config_json)
        .map_err(|error| ChannelBridgeError::new(error.to_string()))?;
    Ok(ChannelProfile { id: row.id, name: row.name, transport, enabled: row.enabled, config })
}

#[cfg(test)]
#[path = "../tests/profile_registry/mod.rs"]
mod tests;
