pub(crate) enum SelfCommand {
    Exit,
    Help,
    Status,
    Compress,
    Handoff { name: String, summary: String },
    Invalid(String),
    Prompt(String),
}

pub(crate) fn parse_self_command(input: &str) -> SelfCommand {
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
                    return SelfCommand::Invalid("usage: /handoff <name> <summary>".to_string());
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

pub(crate) fn print_help() {
    println!(
        "[self] commands: /help, /exit, /quit, /status, /compress, /handoff <name> <summary>"
    );
}

#[cfg(test)]
mod tests {
    use super::{SelfCommand, parse_self_command};

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
