use agent_core::{CoreError, QuestionRequest, ToolCall};

use super::helpers::next_question_request_id;

pub(super) fn is_question_tool_call(call: &ToolCall) -> bool {
    call.tool_name == "Question"
}

pub(super) fn question_request_from_tool_call(
    call: &ToolCall,
    turn_id: &str,
) -> Result<QuestionRequest, CoreError> {
    builtin_tools::question_request_from_call(call, turn_id, next_question_request_id())
}
