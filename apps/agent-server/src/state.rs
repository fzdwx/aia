use std::sync::{Arc, RwLock};

use provider_registry::ProviderRegistry;
use tokio::sync::broadcast;

use crate::{
    runtime_worker::{ProviderInfoSnapshot, RuntimeWorkerHandle},
    sse::SsePayload,
};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub worker: RuntimeWorkerHandle,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
}
