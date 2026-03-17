use agent_core::ToolCall;

pub(crate) const SERIAL_TOOL_NAMES: &[&str] = &["shell", "write", "edit", "tape_info", "tape_handoff"];

pub(crate) fn can_run_in_parallel(call: &ToolCall) -> bool {
    !SERIAL_TOOL_NAMES.contains(&call.tool_name.as_str())
}
