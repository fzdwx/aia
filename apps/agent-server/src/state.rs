use std::sync::{Arc, Mutex};

use agent_runtime::{AgentRuntime, RuntimeSubscriberId};
use provider_registry::ProviderRegistry;
use tokio::sync::broadcast;

use crate::{model::ServerModel, sse::SsePayload};

pub type SharedState = Arc<Mutex<AppState>>;

pub struct AppState {
    pub runtime: AgentRuntime<ServerModel, agent_core::ToolRegistry>,
    pub subscriber: RuntimeSubscriberId,
    pub session_path: std::path::PathBuf,
    pub registry: ProviderRegistry,
    pub store_path: std::path::PathBuf,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
}
