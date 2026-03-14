mod model;
mod routes;
mod sse;
mod state;

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use tower_http::cors::CorsLayer;

use agent_runtime::AgentRuntime;
use builtin_tools::build_tool_registry;
use provider_registry::ProviderRegistry;
use session_tape::{SessionTape, default_session_path};

use model::{ProviderLaunchChoice, build_model_from_selection};
use state::AppState;

const SERVER_DEFAULT_MAX_TOOL_CALLS_PER_TURN: usize = 50;

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
    let workspace_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("workspace 根目录获取失败: {error}");
            return;
        }
    };

    let registry = ProviderRegistry::load_or_default(&store_path).expect("provider 注册表加载失败");
    let tape = SessionTape::load_jsonl_or_default(&session_path).expect("session 磁带加载失败");

    let selection = choose_provider(&registry, &tape);
    let (identity, model) = build_model_from_selection(selection).expect("模型构建失败");

    let tools = build_tool_registry();

    let mut runtime = AgentRuntime::with_tape(model, tools, identity, tape)
        .with_instructions("你是 aia 的助手。给出清晰、结构化的答案。")
        .with_workspace_root(workspace_root)
        .with_max_tool_calls_per_turn(SERVER_DEFAULT_MAX_TOOL_CALLS_PER_TURN);

    let subscriber = runtime.subscribe();
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(512);

    let state = Arc::new(Mutex::new(AppState {
        runtime,
        subscriber,
        session_path,
        registry,
        store_path,
        broadcast_tx,
    }));

    let app = Router::new()
        .route("/api/providers", get(routes::get_providers))
        .route("/api/providers/list", get(routes::list_providers))
        .route("/api/providers", post(routes::create_provider))
        .route("/api/providers/{name}", put(routes::update_provider))
        .route("/api/providers/{name}", delete(routes::delete_provider))
        .route("/api/providers/switch", post(routes::switch_provider))
        .route("/api/session/history", get(routes::get_history))
        .route("/api/events", get(routes::events))
        .route("/api/turn", post(routes::submit_turn))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3434").await.expect("端口 3434 绑定失败");
    println!("agent-server listening on http://localhost:3434");

    axum::serve(listener, app).await.expect("服务器启动失败");
}
