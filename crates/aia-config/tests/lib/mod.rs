use std::path::{Path, PathBuf};

use super::{
    AIA_DIR_NAME, APP_NAME, DEFAULT_SERVER_BASE_URL, DEFAULT_SERVER_BIND_ADDR,
    DEFAULT_SERVER_EVENT_BUFFER, DEFAULT_SERVER_REQUEST_TIMEOUT_MS, DEFAULT_SESSION_TITLE,
    PROMPT_CACHE_KEY_PREFIX, PROVIDERS_FILE_NAME, SESSION_TAPE_FILE_NAME, SESSIONS_DIR_NAME,
    SPAN_ID_PREFIX, STORE_FILE_NAME, TRACE_ATTR_FIRST_REASONING_DELTA_MS,
    TRACE_ATTR_FIRST_TEXT_DELTA_MS, TRACE_ID_PREFIX, aia_dir_path, build_prompt_cache_key,
    build_request_span_id, build_root_span_id, build_tool_span_id, build_trace_id,
    build_user_agent, default_registry_path, default_session_tape_path, default_sessions_dir,
    default_store_path, sessions_dir_from_registry_path, store_path_from_registry_path,
};

#[test]
fn default_paths_are_under_hidden_workspace_dir() {
    assert_eq!(aia_dir_path(), PathBuf::from(AIA_DIR_NAME));
    assert_eq!(default_registry_path(), PathBuf::from(".aia/providers.json"));
    assert_eq!(default_session_tape_path(), PathBuf::from(".aia/session.jsonl"));
    assert_eq!(default_store_path(), PathBuf::from(".aia/store.sqlite3"));
    assert_eq!(default_sessions_dir(), PathBuf::from(".aia/sessions"));
    assert_eq!(APP_NAME, "aia");
    assert_eq!(DEFAULT_SESSION_TITLE, "New session");
    assert_eq!(DEFAULT_SERVER_BIND_ADDR, "0.0.0.0:3434");
    assert_eq!(DEFAULT_SERVER_BASE_URL, "http://localhost:3434");
    assert_eq!(DEFAULT_SERVER_EVENT_BUFFER, 512);
    assert_eq!(DEFAULT_SERVER_REQUEST_TIMEOUT_MS, 300_000);
    assert_eq!(PROVIDERS_FILE_NAME, "providers.json");
    assert_eq!(SESSION_TAPE_FILE_NAME, "session.jsonl");
    assert_eq!(STORE_FILE_NAME, "store.sqlite3");
    assert_eq!(SESSIONS_DIR_NAME, "sessions");
    assert_eq!(TRACE_ID_PREFIX, "aia-trace");
    assert_eq!(SPAN_ID_PREFIX, "aia-span");
    assert_eq!(PROMPT_CACHE_KEY_PREFIX, "aia");
    assert_eq!(TRACE_ATTR_FIRST_REASONING_DELTA_MS, "aia.first_reasoning_delta_ms");
    assert_eq!(TRACE_ATTR_FIRST_TEXT_DELTA_MS, "aia.first_text_delta_ms");
}

#[test]
fn derived_paths_follow_registry_parent() {
    let registry_path = Path::new("/tmp/aia/.aia/providers.json");
    assert_eq!(
        sessions_dir_from_registry_path(registry_path),
        PathBuf::from("/tmp/aia/.aia/sessions")
    );
    assert_eq!(
        store_path_from_registry_path(registry_path),
        PathBuf::from("/tmp/aia/.aia/store.sqlite3")
    );
}

#[test]
fn user_agent_contains_app_platform_and_version() {
    let user_agent = build_user_agent(APP_NAME, "0.1.0");
    assert!(user_agent.starts_with("aia-"));
    assert!(user_agent.ends_with("/0.1.0"));
}

#[test]
fn trace_and_prompt_helpers_build_stable_identifiers() {
    assert_eq!(build_trace_id("run-1"), "aia-trace-run-1");
    assert_eq!(build_root_span_id("run-1"), "aia-span-run-1-root");
    assert_eq!(build_request_span_id("run-1", "chat", 2), "aia-span-run-1-chat-2");
    assert_eq!(build_tool_span_id("run-1", "call_1"), "aia-span-run-1-tool-call_1");
    assert_eq!(
        build_prompt_cache_key("openai", "gpt-4.1-mini", "session-1"),
        "aia:openai:gpt-4.1-mini:session:session-1"
    );
}
