use axum::{
    Router,
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde::Serialize;

use crate::state::SharedState;
use session_tape::SessionProviderBinding;

#[derive(Deserialize)]
pub(crate) struct CreateSessionRequest {
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct HandoffRequest {
    pub name: String,
    pub summary: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AutoCompressRequest {
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionQuery {
    pub session_id: Option<String>,
    pub before_turn_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct SessionSettingsResponse {
    pub provider: String,
    pub model: String,
    pub protocol: String,
    pub reasoning_effort: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpdateSessionSettingsRequest {
    pub session_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

impl SessionSettingsResponse {
    pub fn from_binding(binding: SessionProviderBinding) -> Self {
        match binding {
            SessionProviderBinding::Bootstrap => Self {
                provider: "bootstrap".into(),
                model: "bootstrap".into(),
                protocol: "bootstrap".into(),
                reasoning_effort: None,
            },
            SessionProviderBinding::Provider {
                name,
                model,
                protocol,
                reasoning_effort,
                ..
            } => Self { provider: name, model, protocol, reasoning_effort },
        }
    }
}

mod handlers;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/sessions", get(handlers::list_sessions).post(handlers::create_session))
        .route("/api/sessions/{id}", delete(handlers::delete_session))
        .route("/api/session/history", get(handlers::get_history))
        .route("/api/session/current-turn", get(handlers::get_current_turn))
        .route("/api/session/info", get(handlers::get_session_info))
        .route(
            "/api/session/settings",
            get(handlers::get_session_settings).put(handlers::update_session_settings),
        )
        .route("/api/session/handoff", post(handlers::create_handoff))
        .route("/api/session/auto-compress", post(handlers::auto_compress_session))
}
