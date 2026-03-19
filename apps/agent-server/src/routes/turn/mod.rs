use axum::{
    Router,
    routing::{get, post},
};

use crate::state::SharedState;

mod dto;
mod handlers;
#[cfg(test)]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/events", get(handlers::events))
        .route("/api/turn", post(handlers::submit_turn))
        .route("/api/turn/cancel", post(handlers::cancel_turn))
}
