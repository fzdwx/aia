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
