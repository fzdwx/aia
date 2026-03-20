use super::{
    EMBEDDED_SELF_PATH, SELF_SESSION_TITLE_PREFIX, build_initial_self_prompt,
    build_self_session_title,
};

#[test]
fn self_prompt_embeds_self_contents() {
    let prompt = build_initial_self_prompt(None);
    assert!(prompt.contains(EMBEDDED_SELF_PATH));
    assert!(prompt.contains("<docs-self-md>"));
    assert!(prompt.contains("直接开始本轮对话"));
}

#[test]
fn self_prompt_appends_startup_task() {
    let prompt = build_initial_self_prompt(Some("stabilize self chat boot flow"));
    assert!(prompt.contains("启动附加任务"));
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
