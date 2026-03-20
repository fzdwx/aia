use channel_bridge::{ChannelProfile, SupportedChannelDefinition};
use serde_json::Value;

use super::ChannelListItem;

pub(crate) fn channel_list_item(
    profile: &ChannelProfile,
    definition: &SupportedChannelDefinition,
) -> ChannelListItem {
    let (config, secret_fields_set) = sanitize_config_for_display(&profile.config, definition);
    ChannelListItem {
        id: profile.id.clone(),
        name: profile.name.clone(),
        transport: profile.transport.clone(),
        enabled: profile.enabled,
        config,
        secret_fields_set,
    }
}

pub(crate) fn merge_channel_config(
    existing: &Value,
    patch: Option<Value>,
    definition: &SupportedChannelDefinition,
) -> Result<Value, String> {
    let mut merged = existing.clone();
    let Some(patch) = patch else {
        return Ok(merged);
    };

    let Some(merged_object) = merged.as_object_mut() else {
        return Err("channel config 必须是对象".into());
    };
    let Some(patch_object) = patch.as_object() else {
        return Err("channel config patch 必须是对象".into());
    };
    let secret_keys = secret_field_keys(definition);

    for (key, value) in patch_object {
        let secret_field = secret_keys.iter().any(|item| item == key);
        if secret_field && value.as_str().is_some_and(|secret| secret.trim().is_empty()) {
            continue;
        }
        merged_object.insert(key.clone(), value.clone());
    }

    Ok(merged)
}

fn sanitize_config_for_display(
    config: &Value,
    definition: &SupportedChannelDefinition,
) -> (Value, Vec<String>) {
    let mut sanitized = config.clone();
    let Some(object) = sanitized.as_object_mut() else {
        return (sanitized, Vec::new());
    };

    let mut secret_fields_set = Vec::new();
    for key in secret_field_keys(definition) {
        let is_set =
            object.get(&key).and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty());
        if is_set {
            secret_fields_set.push(key.clone());
        }
        object.insert(key, Value::String(String::new()));
    }

    (sanitized, secret_fields_set)
}

fn secret_field_keys(definition: &SupportedChannelDefinition) -> Vec<String> {
    definition
        .config_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .filter_map(|(key, schema)| {
                    schema
                        .get("x-secret")
                        .and_then(Value::as_bool)
                        .is_some_and(|secret| secret)
                        .then_some(key.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}
