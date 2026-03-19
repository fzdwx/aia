use std::sync::Arc;

use agent_store::AiaStore;
use channel_bridge::{
    ChannelAdapterCatalog, ChannelRuntimeHost, ChannelRuntimeSupervisor, SupportedChannelDefinition,
};
use channel_feishu::build_feishu_runtime_adapter;

use crate::{
    session_manager::{SessionManagerHandle, read_lock},
    sse::SsePayload,
    state::AppState,
};

use super::host::AgentServerChannelHost;

pub(crate) fn build_channel_adapter_catalog(
    store: Arc<AiaStore>,
    session_manager: SessionManagerHandle,
    broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
) -> ChannelAdapterCatalog {
    let host: Arc<dyn ChannelRuntimeHost> =
        Arc::new(AgentServerChannelHost::new(store, session_manager, broadcast_tx));
    let mut catalog = ChannelAdapterCatalog::new();
    catalog.register(build_feishu_runtime_adapter(host));
    catalog
}

pub(crate) fn build_channel_runtime(catalog: ChannelAdapterCatalog) -> ChannelRuntimeSupervisor {
    ChannelRuntimeSupervisor::new(catalog)
}

pub(crate) fn supported_channel_definitions(
    catalog: &ChannelAdapterCatalog,
) -> Vec<SupportedChannelDefinition> {
    catalog.definitions()
}

pub(crate) async fn sync_channel_runtime(state: &AppState) -> Result<(), String> {
    let profile_registry = read_lock(&state.channel_profile_registry_snapshot).clone();
    let desired_profiles = profile_registry
        .channels()
        .iter()
        .filter(|profile| profile.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let mut runtime = state.channel_runtime.lock().await;
    runtime.sync(desired_profiles).map_err(|error| error.to_string())
}
