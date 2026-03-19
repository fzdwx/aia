use std::sync::{Arc, RwLock};

use agent_store::AiaStore;
use channel_bridge::{ChannelAdapterCatalog, ChannelProfileRegistry, ChannelRuntimeSupervisor};
use tokio::sync::broadcast;

use crate::{
    routes::ProviderRouteService,
    session_manager::{ProviderInfoSnapshot, SessionManagerHandle},
    sse::SsePayload,
};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub session_manager: SessionManagerHandle,
    pub provider_routes: ProviderRouteService,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub channel_profile_registry_snapshot: Arc<RwLock<ChannelProfileRegistry>>,
    pub channel_mutation_lock: Arc<tokio::sync::Mutex<()>>,
    pub store: Arc<AiaStore>,
    pub channel_adapter_catalog: Arc<ChannelAdapterCatalog>,
    pub channel_runtime: Arc<tokio::sync::Mutex<ChannelRuntimeSupervisor>>,
}
