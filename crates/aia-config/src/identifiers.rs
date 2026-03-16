pub const APP_NAME: &str = "aia";
pub const TRACE_ID_PREFIX: &str = "aia-trace";
pub const SPAN_ID_PREFIX: &str = "aia-span";
pub const PROMPT_CACHE_KEY_PREFIX: &str = "aia";
pub const TRACE_ATTR_FIRST_REASONING_DELTA_MS: &str = "aia.first_reasoning_delta_ms";
pub const TRACE_ATTR_FIRST_TEXT_DELTA_MS: &str = "aia.first_text_delta_ms";

pub fn build_trace_id(run_id: &str) -> String {
    format!("{TRACE_ID_PREFIX}-{run_id}")
}

pub fn build_root_span_id(run_id: &str) -> String {
    format!("{SPAN_ID_PREFIX}-{run_id}-root")
}

pub fn build_request_span_id(run_id: &str, request_kind: &str, step_index: u32) -> String {
    format!("{SPAN_ID_PREFIX}-{run_id}-{request_kind}-{step_index}")
}

pub fn build_tool_span_id(run_id: &str, invocation_id: &str) -> String {
    format!("{SPAN_ID_PREFIX}-{run_id}-tool-{invocation_id}")
}

pub fn build_prompt_cache_key(provider_name: &str, model_id: &str, session_id: &str) -> String {
    format!("{PROMPT_CACHE_KEY_PREFIX}:{provider_name}:{model_id}:session:{session_id}")
}
