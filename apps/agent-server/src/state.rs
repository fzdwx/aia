use std::sync::{Arc, Mutex};

use agent_runtime::{AgentRuntime, RuntimeSubscriberId};
use tokio::sync::broadcast;

use crate::{model::ServerModel, sse::SsePayload};

pub type SharedState = Arc<Mutex<AppState>>;

pub struct AppState {
    pub runtime: AgentRuntime<ServerModel, agent_core::ToolRegistry>,
    pub subscriber: RuntimeSubscriberId,
    pub session_path: std::path::PathBuf,
    pub provider_name: String,
    pub model_name: String,
    pub broadcast_tx: broadcast::Sender<SsePayload>,
}
