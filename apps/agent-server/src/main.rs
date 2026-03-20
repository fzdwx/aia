use agent_server::{ServerInitError, bootstrap_state, run_self_chat, run_server};

use crate::cli::{CliCommand, cli_usage, parse_cli_command};

mod cli;

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
        CliCommand::SelfChat { startup_task } => run_self_chat(state, startup_task).await,
    }
}
