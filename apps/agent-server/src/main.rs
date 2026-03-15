mod model;
mod routes;
mod runtime_worker;
mod session_manager;
mod sse;
mod state;

use std::sync::{Arc, RwLock};

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use aia_store::{AiaStore, SessionRecord, generate_session_id, iso8601_now};
use tower_http::cors::CorsLayer;

use provider_registry::ProviderRegistry;
use session_tape::SessionTape;

use model::{ProviderLaunchChoice, build_model_from_selection};
use session_manager::{
    ProviderInfoSnapshot, SessionManagerConfig, spawn_session_manager,
};
use state::AppState;

fn default_aia_store_path() -> std::path::PathBuf {
    provider_registry::default_registry_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("store.sqlite3")
}

fn default_sessions_dir() -> std::path::PathBuf {
    provider_registry::default_registry_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("sessions")
}

fn old_session_path() -> std::path::PathBuf {
    session_tape::default_session_path()
}

/// Migrate legacy .aia/session.jsonl to sessions/{id}.jsonl + SQLite record
fn migrate_legacy_session(
    store: &AiaStore,
    sessions_dir: &std::path::Path,
    registry: &ProviderRegistry,
) {
    let legacy_path = old_session_path();
    if !legacy_path.exists() {
        return;
    }

    // Check if we already have sessions (don't re-migrate)
    if let Ok(existing) = store.list_sessions() {
        if !existing.is_empty() {
            return;
        }
    }

    let session_id = generate_session_id();
    let now = iso8601_now();

    // Ensure sessions dir exists
    if let Err(e) = std::fs::create_dir_all(sessions_dir) {
        eprintln!("sessions 目录创建失败: {e}");
        return;
    }

    // Move file
    let new_path = sessions_dir.join(format!("{session_id}.jsonl"));
    if let Err(e) = std::fs::rename(&legacy_path, &new_path) {
        // If rename fails (cross-device), try copy + delete
        if let Err(e2) = std::fs::copy(&legacy_path, &new_path) {
            eprintln!("session 迁移失败: rename={e}, copy={e2}");
            return;
        }
        let _ = std::fs::remove_file(&legacy_path);
    }

    // Get model from the tape or registry
    let model_name = if let Ok(tape) = SessionTape::load_jsonl_or_default(&new_path) {
        tape.latest_provider_binding()
            .and_then(|b| match b {
                session_tape::SessionProviderBinding::Provider { model, .. } => Some(model),
                _ => None,
            })
            .or_else(|| {
                registry
                    .active_provider()
                    .and_then(|p| p.active_model.clone())
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    let record = SessionRecord {
        id: session_id,
        title: "Default session".to_string(),
        created_at: now.clone(),
        updated_at: now,
        model: model_name,
    };

    if let Err(e) = store.create_session(&record) {
        eprintln!("session 迁移 DB 写入失败: {e}");
    }
}

/// Migrate legacy .aia/llm-traces.sqlite3 and .aia/sessions.sqlite3 into unified store.sqlite3
fn migrate_legacy_db(store: &AiaStore, aia_dir: &std::path::Path) {
    let old_traces = aia_dir.join("llm-traces.sqlite3");
    if old_traces.exists() {
        match store.migrate_from_legacy_file(&old_traces, "llm_request_traces") {
            Ok(()) => {
                let _ = std::fs::remove_file(&old_traces);
            }
            Err(e) => eprintln!("trace 数据迁移失败: {e}"),
        }
    }

    let old_sessions = aia_dir.join("sessions.sqlite3");
    if old_sessions.exists() {
        match store.migrate_from_legacy_file(&old_sessions, "sessions") {
            Ok(()) => {
                let _ = std::fs::remove_file(&old_sessions);
            }
            Err(e) => eprintln!("session 数据迁移失败: {e}"),
        }
    }
}

#[tokio::main]
async fn main() {
    let registry_path = provider_registry::default_registry_path();
    let aia_store_path = default_aia_store_path();
    let sessions_dir = default_sessions_dir();
    let workspace_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("workspace 根目录获取失败: {error}");
            return;
        }
    };

    let registry = ProviderRegistry::load_or_default(&registry_path).expect("provider 注册表加载失败");

    // Initialize unified store (traces + sessions in one DB)
    let store = Arc::new(AiaStore::new(&aia_store_path).expect("数据库初始化失败"));

    // Migrate legacy separate DB files into unified store
    if let Some(aia_dir) = aia_store_path.parent() {
        migrate_legacy_db(&store, aia_dir);
    }

    // Ensure sessions directory exists
    std::fs::create_dir_all(&sessions_dir).expect("sessions 目录创建失败");

    // Migrate legacy session.jsonl if needed
    migrate_legacy_session(&store, &sessions_dir, &registry);

    // If no sessions exist, create a default one
    if store.list_sessions().map(|s| s.is_empty()).unwrap_or(true) {
        let session_id = generate_session_id();
        let now = iso8601_now();
        let model_name = registry
            .active_provider()
            .and_then(|p| p.active_model.clone())
            .unwrap_or_default();
        let record = SessionRecord {
            id: session_id,
            title: "New session".to_string(),
            created_at: now.clone(),
            updated_at: now,
            model: model_name,
        };
        store.create_session(&record).expect("默认 session 创建失败");
    }

    // Determine initial model
    let selection = registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap);

    let (identity, _model) =
        build_model_from_selection(selection, Some(store.clone())).expect("模型构建失败");

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(512);
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
        // Session management
        .route("/api/sessions", get(routes::list_sessions))
        .route("/api/sessions", post(routes::create_session))
        .route("/api/sessions/{id}", delete(routes::delete_session))
        // Per-session endpoints
        .route("/api/session/history", get(routes::get_history))
        .route("/api/session/current-turn", get(routes::get_current_turn))
        .route("/api/session/info", get(routes::get_session_info))
        .route("/api/session/handoff", post(routes::create_handoff))
        .route("/api/events", get(routes::events))
        .route("/api/turn", post(routes::submit_turn))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3434").await.expect("端口 3434 绑定失败");
    println!("agent-server listening on http://localhost:3434");

    axum::serve(listener, app).await.expect("服务器启动失败");
}
