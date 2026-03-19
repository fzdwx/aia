use axum::{
    Router,
    routing::{get, post, put},
};

use crate::state::SharedState;

mod dto;
mod handlers;
#[cfg(test)]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/providers", get(handlers::get_providers).post(handlers::create_provider))
        .route("/api/providers/list", get(handlers::list_providers))
        .route(
            "/api/providers/{name}",
            put(handlers::update_provider).delete(handlers::delete_provider),
        )
        .route("/api/providers/switch", post(handlers::switch_provider))
}
