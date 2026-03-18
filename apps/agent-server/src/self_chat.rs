use std::{
    io::Write,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::StreamEvent;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::broadcast,
};

use crate::{
    bootstrap::ServerInitError,
    routes::prepare_session_for_turn,
    sse::{SsePayload, TurnStatus},
    state::AppState,
};

const SELF_SESSION_TITLE_PREFIX: &str = "Self evolution";

pub async fn run_self_chat(state: Arc<AppState>) -> Result<(), ServerInitError> {
    let self_prompt = load_self_prompt().await?;
    let mut events = state.broadcast_tx.subscribe();
    let session = state
        .session_manager
        .create_session(Some(build_self_session_title()))
        .await
        .map_err(|error| ServerInitError::new("self session 创建", error.message))?;

    let provider_info = state
        .provider_info_snapshot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    println!("[self] session: {}", session.id);
    println!("[self] provider: {}/{}", provider_info.name, provider_info.model);
    print_help();

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

        match parse_self_command(prompt) {
            SelfCommand::Exit => break,
            SelfCommand::Help => {
                print_help();
                continue;
            }
            SelfCommand::Status => {
                print_session_status(&state, &session.id).await?;
                continue;
            }
            SelfCommand::Compress => {
                run_manual_compress(&state, &session.id).await?;
                continue;
            }
            SelfCommand::Handoff { name, summary } => {
                run_handoff(&state, &session.id, name, summary).await?;
                continue;
            }
            SelfCommand::Invalid(message) => {
                eprintln!("{message}");
                continue;
            }
            SelfCommand::Prompt(prompt) => {
                submit_prompt_and_wait(&state, &mut events, &session.id, prompt).await?;
            }
        }
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

pub(crate) fn build_self_prompt(path: &Path, content: &str) -> String {
    format!(
        "请先完整阅读 `{}` 的内容，并把它当作当前自我进化对话的工作约束。不要复述整份文件，只需吸收它，然后直接开始本轮对话。\n\n```md\n{}\n```",
        path.display(),
        content.trim()
    )
}

fn build_self_session_title() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{SELF_SESSION_TITLE_PREFIX} {timestamp}")
}

enum SelfCommand {
    Exit,
    Help,
    Status,
    Compress,
    Handoff { name: String, summary: String },
    Invalid(String),
    Prompt(String),
}

fn parse_self_command(input: &str) -> SelfCommand {
    match input {
        "/exit" | "/quit" => SelfCommand::Exit,
        "/help" => SelfCommand::Help,
        "/status" => SelfCommand::Status,
        "/compress" => SelfCommand::Compress,
        _ => {
            if input == "/status" || input.starts_with("/status ") {
                return SelfCommand::Invalid("usage: /status".to_string());
            }
            if input == "/compress" || input.starts_with("/compress ") {
                return SelfCommand::Invalid("usage: /compress".to_string());
            }
            if input == "/exit"
                || input.starts_with("/exit ")
                || input == "/quit"
                || input.starts_with("/quit ")
            {
                return SelfCommand::Invalid("usage: /exit | /quit".to_string());
            }
            if input == "/help" || input.starts_with("/help ") {
                return SelfCommand::Invalid("usage: /help".to_string());
            }
            if input == "/handoff" || input.starts_with("/handoff") {
                let Some(rest) = input.strip_prefix("/handoff ") else {
                    return SelfCommand::Invalid(
                        "usage: /handoff <name> <summary>".to_string(),
                    );
                };
                let trimmed = rest.trim();
                if let Some((name, summary)) = trimmed.split_once(' ') {
                    let handoff_name = name.trim();
                    let handoff_summary = summary.trim();
                    if !handoff_name.is_empty() && !handoff_summary.is_empty() {
                        return SelfCommand::Handoff {
                            name: handoff_name.to_string(),
                            summary: handoff_summary.to_string(),
                        };
                    }
                }
                return SelfCommand::Invalid("usage: /handoff <name> <summary>".to_string());
            }
            SelfCommand::Prompt(input.to_string())
        }
    }
}

fn print_help() {
    println!("[self] commands: /help, /exit, /quit, /status, /compress, /handoff <name> <summary>");
}

async fn print_session_status(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<(), ServerInitError> {
    let stats = state
        .session_manager
        .get_session_info(session_id.to_string())
        .await
        .map_err(|error| ServerInitError::new("session 状态读取", error.message))?;
    let pressure = stats
        .pressure_ratio
        .map(|ratio| format!("{:.1}%", ratio * 100.0))
        .unwrap_or_else(|| "unknown".to_string());
    let input_tokens = stats
        .last_input_tokens
        .map(|tokens| tokens.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!(
        "[status] entries={} anchors={} since_last_anchor={} input_tokens={} pressure={}",
        stats.total_entries,
        stats.anchor_count,
        stats.entries_since_last_anchor,
        input_tokens,
        pressure
    );
    Ok(())
}

async fn run_manual_compress(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<(), ServerInitError> {
    let compressed = state
        .session_manager
        .auto_compress_session(session_id.to_string())
        .await
        .map_err(|error| ServerInitError::new("手动压缩", error.message))?;

    if compressed {
        println!("[compress] ok");
    } else {
        println!("[compress] skipped");
    }
    Ok(())
}

async fn run_handoff(
    state: &Arc<AppState>,
    session_id: &str,
    name: String,
    summary: String,
) -> Result<(), ServerInitError> {
    let entry_id = state
        .session_manager
        .create_handoff(session_id.to_string(), name.clone(), summary)
        .await
        .map_err(|error| ServerInitError::new("handoff 创建", error.message))?;
    println!("[handoff] {name} -> entry {entry_id}");
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        SELF_SESSION_TITLE_PREFIX, SelfCommand, build_self_prompt, build_self_session_title,
        parse_self_command,
    };

    #[test]
    fn self_prompt_wraps_docs_self_contents() {
        let prompt = build_self_prompt(Path::new("docs/self.md"), "hello self");
        assert!(prompt.contains("docs/self.md"));
        assert!(prompt.contains("hello self"));
        assert!(prompt.contains("直接开始本轮对话"));
    }

    #[test]
    fn self_session_title_includes_timestamp_suffix() {
        let title = build_self_session_title();
        assert!(title.starts_with(SELF_SESSION_TITLE_PREFIX));

        let suffix = title
            .strip_prefix(SELF_SESSION_TITLE_PREFIX)
            .expect("title should keep self prefix")
            .trim();
        assert!(!suffix.is_empty());
        assert!(suffix.parse::<u64>().is_ok());
    }

    #[test]
    fn parse_self_command_understands_builtins() {
        assert!(matches!(parse_self_command("/exit"), SelfCommand::Exit));
        assert!(matches!(parse_self_command("/quit"), SelfCommand::Exit));
        assert!(matches!(parse_self_command("/help"), SelfCommand::Help));
        assert!(matches!(parse_self_command("/status"), SelfCommand::Status));
        assert!(matches!(parse_self_command("/compress"), SelfCommand::Compress));
    }

    #[test]
    fn parse_self_command_extracts_handoff_arguments() {
        let command = parse_self_command("/handoff wake-up summarize latest work");
        match command {
            SelfCommand::Handoff { name, summary } => {
                assert_eq!(name, "wake-up");
                assert_eq!(summary, "summarize latest work");
            }
            _ => panic!("expected handoff command"),
        }
    }

    #[test]
    fn parse_self_command_keeps_unknown_slash_input_as_prompt() {
        let command = parse_self_command("/unknown hello");
        match command {
            SelfCommand::Prompt(prompt) => assert_eq!(prompt, "/unknown hello"),
            _ => panic!("expected plain prompt"),
        }
    }

    #[test]
    fn parse_self_command_rejects_malformed_builtin_usage() {
        assert!(matches!(
            parse_self_command("/handoff"),
            SelfCommand::Invalid(message) if message == "usage: /handoff <name> <summary>"
        ));
        assert!(matches!(
            parse_self_command("/handoff wake-up"),
            SelfCommand::Invalid(message) if message == "usage: /handoff <name> <summary>"
        ));
        assert!(matches!(
            parse_self_command("/status now"),
            SelfCommand::Invalid(message) if message == "usage: /status"
        ));
    }
}
