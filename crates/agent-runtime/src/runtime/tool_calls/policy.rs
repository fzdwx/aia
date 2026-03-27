use agent_core::ToolCall;

pub(crate) const SERIAL_TOOL_NAMES: &[&str] =
    &["write", "edit", "apply_patch", "TapeInfo", "TapeHandoff"];

pub(crate) fn can_run_in_parallel(call: &ToolCall) -> bool {
    !SERIAL_TOOL_NAMES.contains(&call.tool_name.as_str())
}
