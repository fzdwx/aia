use std::{io::Write, sync::Arc};

use agent_core::StreamEvent;
use tokio::sync::broadcast;

use crate::{
    bootstrap::ServerInitError,
    routes::prepare_session_for_turn,
    sse::{SsePayload, TurnStatus},
    state::AppState,
};

pub(crate) async fn print_session_status(
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

pub(crate) async fn run_manual_compress(
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

pub(crate) async fn run_handoff(
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

pub(crate) async fn submit_prompt_and_wait(
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
