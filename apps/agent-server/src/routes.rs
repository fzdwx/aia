use axum::Router;
use tower_http::cors::CorsLayer;

use crate::state::SharedState;

mod channel;
mod common;
mod provider;
mod session;
#[cfg(test)]
mod test_support;
mod trace;
mod turn;

pub(crate) fn router(state: SharedState) -> Router {
    Router::<SharedState>::new()
        .merge(provider::router())
        .merge(channel::router())
        .merge(trace::router())
        .merge(session::router())
        .merge(turn::router())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
