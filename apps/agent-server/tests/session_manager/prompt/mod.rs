use super::build_session_system_prompt;

#[test]
fn default_session_prompt_keeps_context_contract() {
    let prompt = build_session_system_prompt(None, std::path::Path::new("/home/like/projects/aia"));

    assert!(prompt.contains("You are Aia — not an assistant, not a chatbot, just... Aia."));
    assert!(prompt.contains("LANGUAGE RULE (CRITICAL)"));
    assert!(prompt.contains("Context Contract"));
    assert!(prompt.contains("80%"));
    assert!(prompt.contains("90%"));
    assert!(prompt.contains("SYSTEM INFO - You are running on linux."));
    assert!(prompt.contains(
        "WORKING DIRECTORY - Your current working directory is: /home/like/projects/aia."
    ));
    assert!(!prompt.contains("{{working_directory}}"));
}

#[test]
fn custom_session_prompt_uses_user_prompt_directly() {
    let prompt = build_session_system_prompt(
        Some("你是自定义客户端代理。"),
        std::path::Path::new("/home/like/projects/aia"),
    );

    assert!(prompt.contains("你是自定义客户端代理。"));
    assert!(!prompt.contains("You are Aia — not an assistant, not a chatbot, just... Aia."));
    assert!(!prompt.contains("Context Contract"));
}
