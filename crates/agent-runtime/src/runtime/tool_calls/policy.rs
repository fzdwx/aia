use agent_core::ToolCall;

pub(crate) const SERIAL_TOOL_NAMES: &[&str] =
    &["write", "edit", "applypatch", "tapeinfo", "tapehandoff"];

fn normalize_tool_name(name: &str) -> String {
    name.chars().filter(|ch| ch.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()
}

pub(crate) fn can_run_in_parallel(call: &ToolCall) -> bool {
    let normalized = normalize_tool_name(&call.tool_name);
    !SERIAL_TOOL_NAMES.contains(&normalized.as_str())
}
