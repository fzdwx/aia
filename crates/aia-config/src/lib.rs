mod identifiers;
mod paths;
mod server;

pub use identifiers::{
    APP_NAME, PROMPT_CACHE_KEY_PREFIX, SPAN_ID_PREFIX, TRACE_ATTR_FIRST_REASONING_DELTA_MS,
    TRACE_ATTR_FIRST_TEXT_DELTA_MS, TRACE_ID_PREFIX, build_prompt_cache_key, build_request_span_id,
    build_root_span_id, build_tool_span_id, build_trace_id,
};
pub use paths::{
    AIA_DIR_NAME, PROVIDERS_FILE_NAME, SESSION_TAPE_FILE_NAME, SESSIONS_DIR_NAME, STORE_FILE_NAME,
    aia_dir_path, default_registry_path, default_session_tape_path, default_sessions_dir,
    default_store_path, sessions_dir_from_registry_path, store_path_from_registry_path,
};
pub use server::{
    DEFAULT_SERVER_BASE_URL, DEFAULT_SERVER_BIND_ADDR, DEFAULT_SERVER_EVENT_BUFFER,
    DEFAULT_SERVER_REQUEST_TIMEOUT_MS, DEFAULT_SESSION_TITLE, build_user_agent,
};

#[cfg(test)]
#[path = "../tests/lib/mod.rs"]
mod tests;
