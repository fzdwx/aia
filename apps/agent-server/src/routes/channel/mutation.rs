use std::sync::Arc;

use agent_store::AiaStore;
use channel_bridge::{ChannelBridgeError, ChannelProfile, ChannelProfileRegistry};

use crate::{channel_host::sync_channel_runtime, state::SharedState};

use super::super::common::{JsonResponse, error_response};

pub(crate) enum ChannelMutation {
    Upsert { next: ChannelProfile, previous: Option<ChannelProfile> },
    Delete { deleted: ChannelProfile },
}

pub(crate) async fn apply_channel_mutation(
    state: &SharedState,
    previous_registry: ChannelProfileRegistry,
    next_registry: ChannelProfileRegistry,
    mutation: ChannelMutation,
) -> Result<(), JsonResponse> {
    persist_channel_mutation(&state.store, &mutation).await.map_err(|error| {
        error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    })?;

    *crate::session_manager::write_lock(&state.channel_profile_registry_snapshot) = next_registry;
    if let Err(sync_error) = sync_channel_runtime(state.as_ref()).await {
        let rollback_error = rollback_channel_mutation(state, previous_registry, mutation).await;
        let message = match rollback_error {
            Ok(()) => format!("channel runtime 同步失败，已回滚：{sync_error}"),
            Err(rollback_error) => {
                format!(
                    "channel runtime 同步失败且回滚失败：{sync_error}；回滚错误：{rollback_error}"
                )
            }
        };
        return Err(error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, message));
    }

    Ok(())
}

async fn persist_channel_mutation(
    store: &Arc<AiaStore>,
    mutation: &ChannelMutation,
) -> Result<(), ChannelBridgeError> {
    match mutation {
        ChannelMutation::Upsert { next, .. } => {
            ChannelProfileRegistry::upsert_into_store(store, next.clone()).await
        }
        ChannelMutation::Delete { deleted } => {
            ChannelProfileRegistry::delete_from_store(store, &deleted.id).await
        }
    }
}

async fn rollback_channel_mutation(
    state: &SharedState,
    previous_registry: ChannelProfileRegistry,
    mutation: ChannelMutation,
) -> Result<(), String> {
    match mutation {
        ChannelMutation::Upsert { previous, next } => match previous {
            Some(previous) => ChannelProfileRegistry::upsert_into_store(&state.store, previous)
                .await
                .map_err(|error| error.to_string())?,
            None => ChannelProfileRegistry::delete_from_store(&state.store, &next.id)
                .await
                .map_err(|error| error.to_string())?,
        },
        ChannelMutation::Delete { deleted } => {
            ChannelProfileRegistry::upsert_into_store(&state.store, deleted)
                .await
                .map_err(|error| error.to_string())?
        }
    }

    *crate::session_manager::write_lock(&state.channel_profile_registry_snapshot) =
        previous_registry;
    sync_channel_runtime(state.as_ref()).await
}
