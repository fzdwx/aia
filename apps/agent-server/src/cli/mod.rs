pub enum CliCommand {
    Serve { bind_addr: Option<String> },
    SelfChat { startup_task: Option<String> },
}

pub fn parse_cli_command(args: impl IntoIterator<Item = String>) -> Result<CliCommand, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [_binary] => Ok(CliCommand::Serve { bind_addr: None }),
        [_binary, flag, bind_addr] if flag == "--bind" => {
            Ok(CliCommand::Serve { bind_addr: Some(bind_addr.clone()) })
        }
        [_binary, command] if command == "self" => Ok(CliCommand::SelfChat { startup_task: None }),
        [_binary, command, task @ ..] if command == "self" => {
            Ok(CliCommand::SelfChat { startup_task: Some(task.join(" ")) })
        }
        [_binary, flag] if flag == "--bind" => Err("missing value for --bind".into()),
        [_binary, command] if command == "-h" || command == "--help" => {
            println!("{}", cli_usage());
            std::process::exit(0);
        }
        [_binary, unknown, ..] => Err(format!("unknown command: {unknown}")),
        [] => Ok(CliCommand::Serve { bind_addr: None }),
    }
}

pub fn cli_usage() -> &'static str {
    "Usage:\n  agent-server                             Start the HTTP+SSE server\n  agent-server --bind <addr>               Start the HTTP+SSE server on a custom bind address\n  agent-server self [task...]              Start terminal self-chat with embedded docs/self.md installed as the system prompt and an optional startup task"
}

#[cfg(test)]
#[path = "../../tests/cli/mod.rs"]
mod tests;
