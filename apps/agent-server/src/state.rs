use std::sync::{Arc, Mutex};

use agent_runtime::{AgentRuntime, RuntimeSubscriberId};

use crate::model::ServerModel;

pub type SharedState = Arc<Mutex<AppState>>;

pub struct AppState {
    pub runtime: AgentRuntime<ServerModel, agent_core::ToolRegistry>,
    pub subscriber: RuntimeSubscriberId,
    pub session_path: std::path::PathBuf,
    pub provider_name: String,
    pub model_name: String,
}
