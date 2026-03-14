use std::sync::{Arc, RwLock};

use llm_trace::LlmTraceStore;
use provider_registry::ProviderRegistry;
use tokio::sync::broadcast;

use crate::{
    runtime_worker::{CurrentTurnSnapshot, ProviderInfoSnapshot, RuntimeWorkerHandle},
    sse::SsePayload,
};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub worker: RuntimeWorkerHandle,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
    pub provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    pub provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    pub history_snapshot: Arc<RwLock<Vec<agent_runtime::TurnLifecycle>>>,
    pub current_turn_snapshot: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    pub trace_store: Arc<dyn LlmTraceStore>,
}
