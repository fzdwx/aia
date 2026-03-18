mod bootstrap;
mod channel_runtime;
mod cli;
mod model;
mod routes;
mod runtime_worker;
mod self_chat;
mod server;
mod session_manager;
mod sse;
mod state;

use bootstrap::{ServerInitError, bootstrap_state};
use cli::{CliCommand, cli_usage, parse_cli_command};
use self_chat::run_self_chat;
use server::run_server;

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
