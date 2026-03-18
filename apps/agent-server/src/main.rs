mod model;
mod routes;
mod runtime_worker;
mod session_manager;
mod sse;
mod state;

use std::{
    fmt,
    io::Write,
    path::Path,
    sync::{Arc, RwLock},
};

use agent_core::StreamEvent;
use agent_store::{AiaStore, SessionRecord, generate_session_id};
use axum::{
    Router,
    routing::{delete, get, post, put},
};
use tower_http::cors::CorsLayer;

use provider_registry::ProviderRegistry;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::broadcast,
};

use model::{ProviderLaunchChoice, build_model_from_selection};
use routes::prepare_session_for_turn;
use session_manager::{ProviderInfoSnapshot, SessionManagerConfig, spawn_session_manager};
use sse::{SsePayload, TurnStatus};
use state::AppState;

const SELF_SESSION_TITLE: &str = "Self evolution";

enum CliCommand {
    Serve,
    SelfChat,
}

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
    let command = match parse_cli_command(std::env::args()) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n\n{}", cli_usage());
            std::process::exit(2);
        }
    };

    if let Err(error) = run(command).await {
        eprintln!("agent-server 启动失败：{error}");
        std::process::exit(1);
    }
}

async fn run(command: CliCommand) -> Result<(), ServerInitError> {
    let state = bootstrap_state().await?;
    match command {
        CliCommand::Serve => run_server(state).await,
        CliCommand::SelfChat => run_self_chat(state).await,
    }
}

async fn bootstrap_state() -> Result<Arc<AppState>, ServerInitError> {
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

    Ok(state)
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/providers", get(routes::get_providers))
        .route("/api/providers/list", get(routes::list_providers))
        .route("/api/traces/overview", get(routes::get_trace_overview))
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
        .with_state(state)
}

async fn run_server(state: Arc<AppState>) -> Result<(), ServerInitError> {
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

async fn run_self_chat(state: Arc<AppState>) -> Result<(), ServerInitError> {
    let self_prompt = load_self_prompt().await?;
    let mut events = state.broadcast_tx.subscribe();
    let session = state
        .session_manager
        .create_session(Some(SELF_SESSION_TITLE.to_string()))
        .await
        .map_err(|error| ServerInitError::new("self session 创建", error.message))?;

    let provider_info = state
        .provider_info_snapshot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    println!("[self] session: {}", session.id);
    println!("[self] provider: {}/{}", provider_info.name, provider_info.model);
    println!("[self] commands: /exit, /quit");

    submit_prompt_and_wait(&state, &mut events, &session.id, self_prompt).await?;

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print!("\nself> ");
        std::io::stdout()
            .flush()
            .map_err(|error| ServerInitError::new("终端输出刷新", error.to_string()))?;

        let Some(line) = stdin
            .next_line()
            .await
            .map_err(|error| ServerInitError::new("终端输入读取", error.to_string()))?
        else {
            println!();
            break;
        };

        let prompt = line.trim();
        if prompt.is_empty() {
            continue;
        }
        if matches!(prompt, "/exit" | "/quit") {
            break;
        }

        submit_prompt_and_wait(&state, &mut events, &session.id, prompt.to_string()).await?;
    }

    Ok(())
}

async fn load_self_prompt() -> Result<String, ServerInitError> {
    let workspace_root = std::env::current_dir()
        .map_err(|error| ServerInitError::new("workspace 根目录获取", error.to_string()))?;
    let self_path = workspace_root.join("docs/self.md");
    let content = tokio::fs::read_to_string(&self_path)
        .await
        .map_err(|error| ServerInitError::new("docs/self.md 读取", error.to_string()))?;
    Ok(build_self_prompt(&self_path, &content))
}

fn build_self_prompt(path: &Path, content: &str) -> String {
    format!(
        "请先完整阅读 `{}` 的内容，并把它当作当前自我进化对话的工作约束。不要复述整份文件，只需吸收它，然后直接开始本轮对话。\n\n```md\n{}\n```",
        path.display(),
        content.trim()
    )
}

async fn submit_prompt_and_wait(
    state: &Arc<AppState>,
    events: &mut broadcast::Receiver<SsePayload>,
    session_id: &str,
    prompt: String,
) -> Result<(), ServerInitError> {
    prepare_session_for_turn(state.as_ref(), session_id)
        .await
        .map_err(|error| ServerInitError::new("turn 预压缩", error.message))?;

    state
        .session_manager
        .submit_turn(session_id.to_string(), prompt)
        .map_err(|error| ServerInitError::new("turn 提交", error.message))?;

    drain_session_events(events, session_id).await
}

async fn drain_session_events(
    events: &mut broadcast::Receiver<SsePayload>,
    session_id: &str,
) -> Result<(), ServerInitError> {
    let mut streamed_text = false;

    loop {
        match events.recv().await {
            Ok(payload) => match payload {
                SsePayload::Stream { session_id: current, event } if current == session_id => {
                    render_stream_event(&event, &mut streamed_text)?;
                }
                SsePayload::Status { session_id: current, status } if current == session_id => {
                    render_status(status, streamed_text)?;
                }
                SsePayload::TurnCompleted { session_id: current, turn } if current == session_id => {
                    if !streamed_text
                        && let Some(message) = turn.assistant_message.as_deref()
                        && !message.is_empty()
                    {
                        println!("{message}");
                    } else if streamed_text {
                        println!();
                    }
                    return Ok(());
                }
                SsePayload::Error { session_id: current, message } if current == session_id => {
                    if streamed_text {
                        println!();
                    }
                    return Err(ServerInitError::new("turn 执行", message));
                }
                SsePayload::TurnCancelled { session_id: current } if current == session_id => {
                    if streamed_text {
                        println!();
                    }
                    println!("[cancelled]");
                    return Ok(());
                }
                _ => {}
            },
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                eprintln!("[self] lagged {} events; waiting for turn completion", skipped);
            }
            Err(broadcast::error::RecvError::Closed) => {
                return Err(ServerInitError::new("事件流读取", "session event channel closed"));
            }
        }
    }
}

fn render_status(status: TurnStatus, streamed_text: bool) -> Result<(), ServerInitError> {
    if streamed_text {
        println!();
    }
    match status {
        TurnStatus::Waiting => println!("[status] waiting"),
        TurnStatus::Thinking => println!("[status] thinking"),
        TurnStatus::Working => println!("[status] working"),
        TurnStatus::Generating => {}
        TurnStatus::Cancelled => println!("[status] cancelled"),
    }
    std::io::stdout()
        .flush()
        .map_err(|error| ServerInitError::new("终端输出刷新", error.to_string()))
}

fn render_stream_event(
    event: &StreamEvent,
    streamed_text: &mut bool,
) -> Result<(), ServerInitError> {
    match event {
        StreamEvent::ThinkingDelta { text } => {
            if !text.is_empty() {
                println!("[thinking] {text}");
            }
        }
        StreamEvent::TextDelta { text } => {
            print!("{text}");
            *streamed_text = true;
        }
        StreamEvent::ToolCallDetected { invocation_id, tool_name, arguments } => {
            if *streamed_text {
                println!();
                *streamed_text = false;
            }
            println!("[tool:detected] {tool_name} #{invocation_id} {arguments}");
        }
        StreamEvent::ToolCallStarted { invocation_id, tool_name, arguments } => {
            if *streamed_text {
                println!();
                *streamed_text = false;
            }
            println!("[tool:start] {tool_name} #{invocation_id} {arguments}");
        }
        StreamEvent::ToolOutputDelta {
            invocation_id,
            stream,
            text,
        } => {
            if *streamed_text {
                println!();
                *streamed_text = false;
            }
            println!("[tool:{stream:?}] #{invocation_id} {text}");
        }
        StreamEvent::ToolCallCompleted {
            invocation_id,
            tool_name,
            failed,
            ..
        } => {
            if *streamed_text {
                println!();
                *streamed_text = false;
            }
            let status = if *failed { "failed" } else { "ok" };
            println!("[tool:done] {tool_name} #{invocation_id} {status}");
        }
        StreamEvent::Log { text } => {
            if *streamed_text {
                println!();
                *streamed_text = false;
            }
            println!("[log] {text}");
        }
        StreamEvent::Done => {}
    }

    std::io::stdout()
        .flush()
        .map_err(|error| ServerInitError::new("终端输出刷新", error.to_string()))
}

fn parse_cli_command(args: impl IntoIterator<Item = String>) -> Result<CliCommand, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [_binary] => Ok(CliCommand::Serve),
        [_binary, command] if command == "self" => Ok(CliCommand::SelfChat),
        [_binary, command] if command == "-h" || command == "--help" => {
            println!("{}", cli_usage());
            std::process::exit(0);
        }
        [_binary, unknown, ..] => Err(format!("unknown command: {unknown}")),
        [] => Ok(CliCommand::Serve),
    }
}

fn cli_usage() -> &'static str {
    "Usage:\n  agent-server        Start the HTTP+SSE server\n  agent-server self   Read docs/self.md and start a terminal self-chat session"
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{CliCommand, build_self_prompt, parse_cli_command};

    #[test]
    fn parse_cli_defaults_to_server_mode() {
        let command = parse_cli_command(["agent-server".to_string()]).expect("cli should parse");
        assert!(matches!(command, CliCommand::Serve));
    }

    #[test]
    fn parse_cli_accepts_self_subcommand() {
        let command =
            parse_cli_command(["agent-server".to_string(), "self".to_string()]).expect("cli should parse");
        assert!(matches!(command, CliCommand::SelfChat));
    }

    #[test]
    fn self_prompt_wraps_docs_self_contents() {
        let prompt = build_self_prompt(Path::new("docs/self.md"), "hello self");
        assert!(prompt.contains("docs/self.md"));
        assert!(prompt.contains("hello self"));
        assert!(prompt.contains("直接开始本轮对话"));
    }
}
