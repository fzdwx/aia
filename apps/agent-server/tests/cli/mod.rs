use super::{CliCommand, parse_cli_command};

#[test]
fn parse_cli_defaults_to_server_mode() {
    let command = parse_cli_command(["agent-server".to_string()]).expect("cli should parse");
    assert!(matches!(command, CliCommand::Serve { bind_addr: None }));
}

#[test]
fn parse_cli_accepts_custom_bind_addr() {
    let command = parse_cli_command([
        "agent-server".to_string(),
        "--bind".to_string(),
        "127.0.0.1:4545".to_string(),
    ])
    .expect("cli should parse");

    assert!(matches!(
        command,
        CliCommand::Serve {
            bind_addr: Some(addr)
        } if addr == "127.0.0.1:4545"
    ));
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
