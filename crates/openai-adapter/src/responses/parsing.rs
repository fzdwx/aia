use agent_core::{Completion, CompletionSegment, ToolCall};

use crate::{OpenAiAdapterError, parse_tool_arguments};

use super::{
    OpenAiResponsesModel,
    payloads::{ReasoningSummaryPart, ResponsesContent, ResponsesOutput, ResponsesResponse},
};

impl OpenAiResponsesModel {
    pub(crate) fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
        let payload: ResponsesResponse = serde_json::from_str(body)
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
        let usage = Self::map_usage(payload.usage.clone());
        let response_id = payload.id.clone();
        let status = payload.status.clone();
        let incomplete_reason =
            payload.incomplete_details.as_ref().and_then(|details| details.reason.clone());

        let mut segments = Vec::new();
        let mut has_tool_calls = false;

        for (index, item) in payload.output.into_iter().enumerate() {
            match item {
                ResponsesOutput::Reasoning { summary } => {
                    if let Some(thinking) = collect_reasoning_summary(summary) {
                        segments.push(CompletionSegment::Thinking(thinking));
                    }
                }
                ResponsesOutput::Message { content } => {
                    segments.extend(content.into_iter().filter_map(message_content_to_segment));
                }
                ResponsesOutput::FunctionCall { id, call_id, name, arguments } => {
                    has_tool_calls = true;
                    let invocation_id =
                        id.or(call_id).unwrap_or_else(|| format!("openai-call-{}", index + 1));
                    segments.push(CompletionSegment::ToolUse(build_tool_call(
                        name,
                        invocation_id,
                        arguments,
                        response_id.clone(),
                    )?));
                }
                ResponsesOutput::Other => {}
            }
        }

        Ok(Completion {
            segments,
            stop_reason: Self::map_stop_reason(
                status.as_deref(),
                incomplete_reason.as_deref(),
                has_tool_calls,
            ),
            usage,
            response_body: Some(body.to_string()),
            http_status_code: None,
        })
    }
}

fn collect_reasoning_summary(summary: Vec<ReasoningSummaryPart>) -> Option<String> {
    let text = summary
        .into_iter()
        .filter_map(|part| match part {
            ReasoningSummaryPart::SummaryText { text } => Some(text),
            ReasoningSummaryPart::Other => None,
        })
        .collect::<Vec<_>>()
        .join("");

    if text.is_empty() { None } else { Some(text) }
}

fn message_content_to_segment(content: ResponsesContent) -> Option<CompletionSegment> {
    match content {
        ResponsesContent::OutputText { text } => Some(CompletionSegment::Text(text)),
        ResponsesContent::Other => None,
    }
}

fn build_tool_call(
    name: String,
    invocation_id: String,
    arguments: String,
    response_id: Option<String>,
) -> Result<ToolCall, OpenAiAdapterError> {
    let mut call = ToolCall::new(name)
        .with_invocation_id(invocation_id)
        .with_arguments_value(parse_tool_arguments(&arguments)?);
    if let Some(response_id) = response_id {
        call = call.with_response_id(response_id);
    }
    Ok(call)
}
