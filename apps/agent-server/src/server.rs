use std::sync::Arc;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use tower_http::cors::CorsLayer;

use crate::{bootstrap::ServerInitError, routes, state::AppState};

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/providers", get(routes::get_providers))
        .route("/api/providers/list", get(routes::list_providers))
        .route("/api/channels", get(routes::list_channels))
        .route("/api/channels/catalog", get(routes::list_supported_channels))
        .route("/api/traces/overview", get(routes::get_trace_overview))
        .route("/api/traces", get(routes::list_traces))
        .route("/api/traces/summary", get(routes::get_trace_summary))
        .route("/api/traces/{id}", get(routes::get_trace))
        .route("/api/providers", post(routes::create_provider))
        .route("/api/channels", post(routes::create_channel))
        .route("/api/providers/{name}", put(routes::update_provider))
        .route("/api/channels/{id}", put(routes::update_channel))
        .route("/api/providers/{name}", delete(routes::delete_provider))
        .route("/api/channels/{id}", delete(routes::delete_channel))
        .route("/api/providers/switch", post(routes::switch_provider))
        .route("/api/sessions", get(routes::list_sessions))
        .route("/api/sessions", post(routes::create_session))
        .route("/api/sessions/{id}", delete(routes::delete_session))
        .route("/api/session/history", get(routes::get_history))
        .route("/api/session/current-turn", get(routes::get_current_turn))
        .route("/api/session/info", get(routes::get_session_info))
        .route("/api/session/handoff", post(routes::create_handoff))
        .route("/api/session/auto-compress", post(routes::auto_compress_session))
        .route("/api/events", get(routes::events))
        .route("/api/turn", post(routes::submit_turn))
        .route("/api/turn/cancel", post(routes::cancel_turn))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn run_server(state: Arc<AppState>) -> Result<(), ServerInitError> {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(aia_config::DEFAULT_SERVER_BIND_ADDR)
        .await
        .map_err(|error| ServerInitError::new("端口 3434 绑定", error.to_string()))?;
    println!("agent-server listening on {}", aia_config::DEFAULT_SERVER_BASE_URL);

    axum::serve(listener, app)
        .await
        .map_err(|error| ServerInitError::new("服务器启动", error.to_string()))?;

    Ok(())
}
