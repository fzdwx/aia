use axum::{
    Router,
    routing::{get, put},
};

use crate::state::SharedState;

mod config;
mod dto;
mod handlers;
mod mutation;
#[cfg(test)]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/channels", get(handlers::list_channels).post(handlers::create_channel))
        .route("/api/channels/catalog", get(handlers::list_supported_channels))
        .route("/api/channels/{id}", put(handlers::update_channel).delete(handlers::delete_channel))
}
