mod commands;
mod prompt;
mod session;

use std::{io::Write, sync::Arc};

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{
    bootstrap::{ServerBootstrapOptions, ServerInitError},
    state::AppState,
};

use self::commands::{SelfCommand, parse_self_command, print_help};
use self::prompt::{
    build_initial_self_message, build_self_chat_system_prompt, build_self_session_title,
};
use self::session::{
    print_session_status, run_handoff, run_manual_compress, submit_prompt_and_wait,
};

pub fn self_chat_bootstrap_options() -> ServerBootstrapOptions {
    ServerBootstrapOptions::default().with_system_prompt(build_self_chat_system_prompt())
}

pub async fn run_self_chat(
    state: Arc<AppState>,
    startup_task: Option<String>,
) -> Result<(), ServerInitError> {
    let initial_message = build_initial_self_message(startup_task.as_deref());
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
    println!("[self] provider: {}/{}", provider_info.provider_id, provider_info.model_id);
    if let Some(task) = startup_task.as_deref() {
        println!("[self] startup task: {task}");
    }
    print_help();

    submit_prompt_and_wait(&state, &mut events, &session.id, initial_message).await?;

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
