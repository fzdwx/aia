use axum::{Router, routing::get};

use crate::state::SharedState;

mod dto;
mod handlers;
#[cfg(test)]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/traces/overview", get(handlers::get_trace_overview))
        .route("/api/traces", get(handlers::list_traces))
        .route("/api/traces/summary", get(handlers::get_trace_summary))
        .route("/api/traces/{id}", get(handlers::get_trace))
}
