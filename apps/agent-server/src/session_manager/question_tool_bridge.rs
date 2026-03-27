use std::collections::BTreeSet;

use agent_core::{PendingToolRequest, QuestionRequest, QuestionResult, ToolCall, ToolResult};
use session_tape::SessionTape;

use crate::runtime_worker::RuntimeWorkerError;

pub(crate) fn question_tool_call(request: &QuestionRequest) -> ToolCall {
    ToolCall::new("Question")
        .with_invocation_id(request.invocation_id.clone())
        .with_arguments_value(serde_json::json!({ "questions": request.questions }))
}

pub(crate) fn question_request_from_pending_tool_request(
    request: &PendingToolRequest,
) -> Result<QuestionRequest, RuntimeWorkerError> {
    if request.kind != "question" {
        return Err(RuntimeWorkerError::bad_request(format!(
            "unsupported pending tool request kind: {}",
            request.kind
        )));
    }

    let decoded: QuestionRequest =
        serde_json::from_value(request.payload.clone()).map_err(|error| {
            RuntimeWorkerError::internal(format!("pending question request decode failed: {error}"))
        })?;

    if decoded.request_id != request.request_id
        || decoded.invocation_id != request.invocation_id
        || decoded.turn_id != request.turn_id
    {
        return Err(RuntimeWorkerError::internal(
            "pending question request payload does not match envelope",
        ));
    }

    Ok(decoded)
}

pub(crate) fn pending_question_request_from_runtime_tape(
    tape: &SessionTape,
) -> Result<Option<QuestionRequest>, RuntimeWorkerError> {
    let mut resolved_invocation_ids = BTreeSet::new();

    for entry in tape.entries().iter().rev() {
        if let Some(result) = entry.as_tool_result() {
            resolved_invocation_ids.insert(result.invocation_id);
            continue;
        }

        if entry.kind == "event" && entry.event_name() == Some("tool_request_pending") {
            let data = entry.event_data().ok_or_else(|| {
                RuntimeWorkerError::internal("tool_request_pending event missing payload")
            })?;
            let pending: PendingToolRequest =
                serde_json::from_value(data.clone()).map_err(|error| {
                    RuntimeWorkerError::internal(format!(
                        "pending tool request decode failed: {error}"
                    ))
                })?;
            if pending.kind != "question"
                || resolved_invocation_ids.contains(&pending.invocation_id)
            {
                continue;
            }
            return question_request_from_pending_tool_request(&pending).map(Some);
        }
    }

    Ok(None)
}

pub(crate) fn question_tool_result(
    call: &ToolCall,
    result: &QuestionResult,
) -> Result<ToolResult, RuntimeWorkerError> {
    let content = serde_json::to_string(result).map_err(|error| {
        RuntimeWorkerError::internal(format!("question result encode failed: {error}"))
    })?;
    let details = serde_json::to_value(result).map_err(|error| {
        RuntimeWorkerError::internal(format!("question result serialize failed: {error}"))
    })?;
    Ok(ToolResult::from_call(call, content).with_details(details))
}
