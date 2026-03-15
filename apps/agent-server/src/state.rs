use std::sync::{Arc, RwLock};

use agent_store::AiaStore;
use provider_registry::ProviderRegistry;
use tokio::sync::broadcast;

use crate::{
    session_manager::{ProviderInfoSnapshot, SessionManagerHandle},
    sse::SsePayload,
};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub session_manager: SessionManagerHandle,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub store: Arc<AiaStore>,
}
