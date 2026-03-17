use agent_runtime::TurnControl;
use axum::http::StatusCode;
use provider_registry::{ModelConfig, ProviderKind};
use serde::{Deserialize, Serialize};

use crate::sse::TurnStatus;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentToolOutput {
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub detected_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub output: String,
    pub completed: bool,
    pub result_content: Option<String>,
    pub result_details: Option<serde_json::Value>,
    pub failed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurrentTurnBlock {
    Thinking { content: String },
    Tool { tool: CurrentToolOutput },
    Text { content: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentTurnSnapshot {
    pub started_at_ms: u64,
    pub user_message: String,
    pub status: TurnStatus,
    pub blocks: Vec<CurrentTurnBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfoSnapshot {
    pub name: String,
    pub model: String,
    pub connected: bool,
}

impl ProviderInfoSnapshot {
    pub fn from_identity(identity: &agent_core::ModelIdentity) -> Self {
        Self { name: identity.provider.clone(), model: identity.name.clone(), connected: true }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeWorkerError {
    pub status: StatusCode,
    pub message: String,
}

impl RuntimeWorkerError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn unavailable() -> Self {
        Self::internal("runtime worker unavailable")
    }
}

#[derive(Clone)]
pub struct CreateProviderInput {
    pub name: String,
    pub kind: ProviderKind,
    pub models: Vec<ModelConfig>,
    pub active_model: Option<String>,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct UpdateProviderInput {
    pub kind: Option<ProviderKind>,
    pub models: Option<Vec<ModelConfig>>,
    pub active_model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct SwitchProviderInput {
    pub name: String,
    pub model_id: Option<String>,
}

#[derive(Clone)]
pub struct RunningTurnHandle {
    pub control: TurnControl,
}
