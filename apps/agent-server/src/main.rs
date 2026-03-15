mod model;
mod routes;
mod runtime_worker;
mod sse;
mod state;

use std::sync::{Arc, RwLock};

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use llm_trace::{LlmTraceStore, SqliteLlmTraceStore};
use tower_http::cors::CorsLayer;

use agent_runtime::AgentRuntime;
use builtin_tools::build_tool_registry;
use provider_registry::ProviderRegistry;
use session_tape::{SessionTape, default_session_path};

use model::{ProviderLaunchChoice, build_model_from_selection};
use runtime_worker::{ProviderInfoSnapshot, RuntimeOwnerState, spawn_runtime_worker};
use state::AppState;

fn default_trace_store_path() -> std::path::PathBuf {
    provider_registry::default_registry_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("llm-traces.sqlite3")
}

fn choose_provider(registry: &ProviderRegistry, tape: &SessionTape) -> ProviderLaunchChoice {
    use session_tape::SessionProviderBinding;

    if let Some(binding) = tape.latest_provider_binding() {
        match binding {
            SessionProviderBinding::Bootstrap => return ProviderLaunchChoice::Bootstrap,
            SessionProviderBinding::Provider { name, model, base_url, protocol } => {
                if let Some(profile) = registry.providers().iter().find(|provider| {
                    provider.name == name
                        && provider.has_model(&model)
                        && provider.base_url == base_url
                        && provider.kind.protocol_name() == protocol.as_str()
                }) {
                    return ProviderLaunchChoice::OpenAi(profile.clone());
                }
            }
        }
    }

    registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap)
}

#[tokio::main]
async fn main() {
    let store_path = provider_registry::default_registry_path();
    let session_path = default_session_path();
    let trace_store_path = default_trace_store_path();
    let workspace_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("workspace 根目录获取失败: {error}");
            return;
        }
    };

    let registry = ProviderRegistry::load_or_default(&store_path).expect("provider 注册表加载失败");
    let tape = SessionTape::load_jsonl_or_default(&session_path).expect("session 磁带加载失败");
    let trace_store: Arc<dyn LlmTraceStore> =
        Arc::new(SqliteLlmTraceStore::new(&trace_store_path).expect("trace 数据库初始化失败"));

    let selection = choose_provider(&registry, &tape);
    let (identity, model) =
        build_model_from_selection(selection, Some(trace_store.clone())).expect("模型构建失败");

    let tools = build_tool_registry();
    let session_append_path = session_path.clone();

    let mut runtime = AgentRuntime::with_tape(model, tools, identity, tape)
        .with_instructions(format!(
            "你是 aia 的助手。给出清晰、结构化的答案。\n\n{}",
            agent_prompts::context_contract(
                agent_prompts::AGENT_HANDOFF_THRESHOLD,
                agent_prompts::AUTO_COMPRESSION_THRESHOLD,
            ),
        ))
        .with_workspace_root(workspace_root)
        .with_tape_entry_listener(move |entry| {
            SessionTape::append_jsonl_entry(&session_append_path, entry)
        })
        .with_max_tool_calls_per_turn(100000);

    let subscriber = runtime.subscribe();
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(512);
    let provider_registry_snapshot = Arc::new(RwLock::new(registry.clone()));
    let provider_info_snapshot =
        Arc::new(RwLock::new(ProviderInfoSnapshot::from_identity(runtime.model_identity())));
    let history_snapshot = Arc::new(RwLock::new(Vec::new()));
    let current_turn_snapshot = Arc::new(RwLock::new(None));
    let worker = spawn_runtime_worker(RuntimeOwnerState {
        runtime,
        subscriber,
        session_path,
        registry,
        store_path,
        trace_store: trace_store.clone(),
        broadcast_tx: broadcast_tx.clone(),
        provider_registry_snapshot: provider_registry_snapshot.clone(),
        provider_info_snapshot: provider_info_snapshot.clone(),
        history_snapshot: history_snapshot.clone(),
        current_turn_snapshot: current_turn_snapshot.clone(),
    });

    let state = Arc::new(AppState {
        worker,
        broadcast_tx,
        provider_registry_snapshot,
        provider_info_snapshot,
        history_snapshot,
        current_turn_snapshot,
        trace_store,
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
