use std::sync::{Arc, RwLock};

use agent_store::AiaStore;
use channel_bridge::{ChannelAdapterCatalog, ChannelProfileRegistry, ChannelRuntimeSupervisor};
use provider_registry::ProviderRegistry;
use tokio::sync::{Mutex, broadcast};

use crate::{
    session_manager::{ProviderInfoSnapshot, SessionManagerHandle},
    sse::SsePayload,
};

pub type SharedState = Arc<AppState>;
pub type Snapshot<T> = Arc<RwLock<T>>;

pub struct AppState {
    pub session_manager: SessionManagerHandle,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Snapshot<ProviderRegistry>,
    pub provider_info_snapshot: Snapshot<ProviderInfoSnapshot>,
    pub channel_profile_registry_snapshot: Snapshot<ChannelProfileRegistry>,
    pub channel_mutation_lock: Arc<Mutex<()>>,
    pub store: Arc<AiaStore>,
    pub channel_adapter_catalog: Arc<ChannelAdapterCatalog>,
    pub channel_runtime: Arc<Mutex<ChannelRuntimeSupervisor>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_manager: SessionManagerHandle,
        broadcast_tx: broadcast::Sender<SsePayload>,
        provider_registry_snapshot: Snapshot<ProviderRegistry>,
        provider_info_snapshot: Snapshot<ProviderInfoSnapshot>,
        channel_profile_registry_snapshot: Snapshot<ChannelProfileRegistry>,
        store: Arc<AiaStore>,
        channel_adapter_catalog: Arc<ChannelAdapterCatalog>,
        channel_runtime: Arc<Mutex<ChannelRuntimeSupervisor>>,
    ) -> Self {
        Self {
            session_manager,
            broadcast_tx,
            provider_registry_snapshot,
            provider_info_snapshot,
            channel_profile_registry_snapshot,
            channel_mutation_lock: Arc::new(Mutex::new(())),
            store,
            channel_adapter_catalog,
            channel_runtime,
        }
    }
}
