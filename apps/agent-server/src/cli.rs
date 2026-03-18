pub enum CliCommand {
    Serve,
    SelfChat,
}

pub fn parse_cli_command(args: impl IntoIterator<Item = String>) -> Result<CliCommand, String> {
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

pub fn cli_usage() -> &'static str {
    "Usage:\n  agent-server        Start the HTTP+SSE server\n  agent-server self   Read docs/self.md and start a terminal self-chat session"
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
        let command =
            parse_cli_command(["agent-server".to_string(), "self".to_string()]).expect("cli should parse");
        assert!(matches!(command, CliCommand::SelfChat));
    }
}
