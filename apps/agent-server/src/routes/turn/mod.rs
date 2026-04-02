use axum::{
    Router,
    routing::{get, post},
};
use serde::Deserialize;

use crate::state::SharedState;

#[derive(Deserialize)]
pub(crate) struct TurnRequest {
    /// 用户消息
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub prompts: Option<Vec<String>>,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CancelTurnRequest {
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RetryTurnRequest {
    pub failed_turn_id: String,
    pub session_id: Option<String>,
}

mod handlers;
#[cfg(test)]
#[path = "../../../tests/routes/turn/mod.rs"]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/events", get(handlers::events))
        .route("/api/turn", post(handlers::submit_turn))
        .route("/api/turn/retry", post(handlers::retry_turn))
        .route("/api/turn/cancel", post(handlers::cancel_turn))
}
