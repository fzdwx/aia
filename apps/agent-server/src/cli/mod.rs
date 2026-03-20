pub enum CliCommand {
    Serve,
    SelfChat { startup_task: Option<String> },
}

pub fn parse_cli_command(args: impl IntoIterator<Item = String>) -> Result<CliCommand, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [_binary] => Ok(CliCommand::Serve),
        [_binary, command] if command == "self" => Ok(CliCommand::SelfChat { startup_task: None }),
        [_binary, command, task @ ..] if command == "self" => {
            Ok(CliCommand::SelfChat { startup_task: Some(task.join(" ")) })
        }
        [_binary, command] if command == "-h" || command == "--help" => {
            println!("{}", cli_usage());
            std::process::exit(0);
        }
        [_binary, unknown, ..] => Err(format!("unknown command: {unknown}")),
        [] => Ok(CliCommand::Serve),
    }
}

pub fn cli_usage() -> &'static str {
    "Usage:\n  agent-server                    Start the HTTP+SSE server\n  agent-server self [task...]    Start terminal self-chat with embedded docs/self.md guidance and an optional startup task"
}

#[cfg(test)]
mod tests {
    use super::{CliCommand, parse_cli_command};

    #[test]
    fn parse_cli_defaults_to_server_mode() {
        let command = parse_cli_command(["agent-server".to_string()]).expect("cli should parse");
        assert!(matches!(command, CliCommand::Serve));
    }

    #[test]
    fn parse_cli_accepts_self_subcommand() {
        let command = parse_cli_command(["agent-server".to_string(), "self".to_string()])
            .expect("cli should parse");
        assert!(matches!(command, CliCommand::SelfChat { startup_task: None }));
    }

    #[test]
    fn parse_cli_collects_self_startup_task() {
        let command = parse_cli_command([
            "agent-server".to_string(),
            "self".to_string(),
            "stabilize".to_string(),
            "self".to_string(),
            "chat".to_string(),
        ])
        .expect("cli should parse");

        assert!(matches!(
            command,
            CliCommand::SelfChat {
                startup_task: Some(task)
            } if task == "stabilize self chat"
        ));
    }
}
