use axum::{
    Router,
    routing::{delete, get, post},
};
use serde::Deserialize;

use crate::state::SharedState;

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

mod handlers;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/sessions", get(handlers::list_sessions).post(handlers::create_session))
        .route("/api/sessions/{id}", delete(handlers::delete_session))
        .route("/api/session/history", get(handlers::get_history))
        .route("/api/session/current-turn", get(handlers::get_current_turn))
        .route("/api/session/info", get(handlers::get_session_info))
        .route("/api/session/handoff", post(handlers::create_handoff))
        .route("/api/session/auto-compress", post(handlers::auto_compress_session))
}
