use axum::{Router, routing::get};
use serde::Deserialize;

use crate::state::SharedState;

#[derive(Deserialize)]
pub(crate) struct TraceListQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub request_kind: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TraceDashboardQuery {
    pub range: Option<String>,
}

mod handlers;
#[cfg(test)]
#[path = "../../../tests/routes/trace/mod.rs"]
mod tests;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/traces/overview", get(handlers::get_trace_overview))
        .route("/api/traces/dashboard", get(handlers::get_trace_dashboard))
        .route("/api/traces", get(handlers::list_traces))
        .route("/api/traces/{id}", get(handlers::get_trace))
}
