use super::{
    EMBEDDED_SELF_PATH, SELF_SESSION_TITLE_PREFIX, build_initial_self_message,
    build_self_chat_system_prompt, build_self_session_title,
};

#[test]
fn self_system_prompt_embeds_self_contents() {
    let prompt = build_self_chat_system_prompt();

    assert!(prompt.contains("autonomous engineering agent"));
    assert!(prompt.contains("docs/evolution-log.md"));
    assert!(!prompt.contains(EMBEDDED_SELF_PATH));
}

#[test]
fn self_initial_message_starts_wake_without_embedding_docs() {
    let prompt = build_initial_self_message(None);

    assert!(prompt.contains("开始本轮 wake"));
    assert!(prompt.contains("system prompt"));
    assert!(!prompt.contains(EMBEDDED_SELF_PATH));
}

#[test]
fn self_initial_message_appends_startup_task() {
    let prompt = build_initial_self_message(Some("stabilize self chat boot flow"));
    assert!(prompt.contains("优先处理这项任务"));
    assert!(prompt.contains("stabilize self chat boot flow"));
}

#[test]
fn self_session_title_includes_timestamp_suffix() {
    let title = build_self_session_title();
    assert!(title.starts_with(SELF_SESSION_TITLE_PREFIX));

    let suffix = title
        .strip_prefix(SELF_SESSION_TITLE_PREFIX)
        .expect("title should keep self prefix")
        .trim();
    assert!(!suffix.is_empty());
    assert!(suffix.parse::<u64>().is_ok());
}
