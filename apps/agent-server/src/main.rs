mod model;
mod routes;
mod runtime_worker;
mod session_manager;
mod sse;
mod state;

use std::{
    fmt,
    sync::{Arc, RwLock},
};

use agent_store::{AiaStore, SessionRecord, generate_session_id};
use axum::{
    Router,
    routing::{delete, get, post, put},
};
use tower_http::cors::CorsLayer;

use provider_registry::ProviderRegistry;

use model::{ProviderLaunchChoice, build_model_from_selection};
use session_manager::{ProviderInfoSnapshot, SessionManagerConfig, spawn_session_manager};
use state::AppState;

fn build_server_user_agent() -> String {
    aia_config::build_user_agent(aia_config::APP_NAME, env!("CARGO_PKG_VERSION"))
}

#[derive(Debug)]
struct ServerInitError {
    step: &'static str,
    message: String,
}

impl ServerInitError {
    fn new(step: &'static str, message: impl Into<String>) -> Self {
        Self { step, message: message.into() }
    }
}

impl fmt::Display for ServerInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{step}失败: {message}", step = self.step, message = self.message)
    }
}

impl std::error::Error for ServerInitError {}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("agent-server 启动失败：{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), ServerInitError> {
    let registry_path = provider_registry::default_registry_path();
    let aia_store_path = aia_config::store_path_from_registry_path(&registry_path);
    let sessions_dir = aia_config::sessions_dir_from_registry_path(&registry_path);
    let workspace_root = std::env::current_dir()
        .map_err(|error| ServerInitError::new("workspace 根目录获取", error.to_string()))?;

    let registry = ProviderRegistry::load_or_default(&registry_path)
        .map_err(|error| ServerInitError::new("provider 注册表加载", error.to_string()))?;

    let store = Arc::new(
        AiaStore::new(&aia_store_path)
            .map_err(|error| ServerInitError::new("数据库初始化", error.to_string()))?,
    );

    std::fs::create_dir_all(&sessions_dir)
        .map_err(|error| ServerInitError::new("sessions 目录创建", error.to_string()))?;

    let first_session_id = store
        .first_session_id_async()
        .await
        .map_err(|error| ServerInitError::new("session 首条记录加载", error.to_string()))?;
    if first_session_id.is_none() {
        let session_id = generate_session_id();
        let model_name = registry
            .active_provider()
            .and_then(|provider| provider.active_model.clone())
            .unwrap_or_default();
        let record = SessionRecord::new(
            session_id,
            aia_config::DEFAULT_SESSION_TITLE.to_string(),
            model_name,
        );
        store
            .create_session_async(record)
            .await
            .map_err(|error| ServerInitError::new("默认 session 创建", error.to_string()))?;
    }

    let selection = registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap);

    let (identity, _model) = build_model_from_selection(selection, Some(store.clone()))
        .map_err(|error| ServerInitError::new("模型构建", error.to_string()))?;

    let (broadcast_tx, _) =
        tokio::sync::broadcast::channel(aia_config::DEFAULT_SERVER_EVENT_BUFFER);
    let provider_registry_snapshot = Arc::new(RwLock::new(registry.clone()));
    let provider_info_snapshot =
        Arc::new(RwLock::new(ProviderInfoSnapshot::from_identity(&identity)));

    let session_manager = spawn_session_manager(SessionManagerConfig {
        sessions_dir,
        store: store.clone(),
        registry,
        store_path: registry_path,
        broadcast_tx: broadcast_tx.clone(),
        provider_registry_snapshot: provider_registry_snapshot.clone(),
        provider_info_snapshot: provider_info_snapshot.clone(),
        workspace_root,
        user_agent: build_server_user_agent(),
    });

    let state = Arc::new(AppState {
        session_manager,
        broadcast_tx,
        provider_registry_snapshot,
        provider_info_snapshot,
        store,
    });

    let app = Router::new()
        .route("/api/providers", get(routes::get_providers))
        .route("/api/providers/list", get(routes::list_providers))
        .route("/api/traces", get(routes::list_traces))
        .route("/api/traces/summary", get(routes::get_trace_summary))
        .route("/api/traces/{id}", get(routes::get_trace))
        .route("/api/providers", post(routes::create_provider))
        .route("/api/providers/{name}", put(routes::update_provider))
        .route("/api/providers/{name}", delete(routes::delete_provider))
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(aia_config::DEFAULT_SERVER_BIND_ADDR)
        .await
        .map_err(|error| ServerInitError::new("端口 3434 绑定", error.to_string()))?;
    println!("agent-server listening on {}", aia_config::DEFAULT_SERVER_BASE_URL);

    axum::serve(listener, app)
        .await
        .map_err(|error| ServerInitError::new("服务器启动", error.to_string()))?;

    Ok(())
}
