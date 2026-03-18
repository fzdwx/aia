use std::{
    io::Write,
    path::Path,
    sync::Arc,
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

const SELF_SESSION_TITLE: &str = "Self evolution";

pub async fn run_self_chat(state: Arc<AppState>) -> Result<(), ServerInitError> {
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

pub(crate) fn build_self_prompt(path: &Path, content: &str) -> String {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::build_self_prompt;

    #[test]
    fn self_prompt_wraps_docs_self_contents() {
        let prompt = build_self_prompt(Path::new("docs/self.md"), "hello self");
        assert!(prompt.contains("docs/self.md"));
        assert!(prompt.contains("hello self"));
        assert!(prompt.contains("直接开始本轮对话"));
    }
}
