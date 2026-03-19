use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::state::SharedState;

mod dto;
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
