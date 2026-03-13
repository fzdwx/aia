use std::io::{self, BufRead};

use agent_core::{
    Completion, CompletionRequest, CompletionSegment, LanguageModel, ModelCheckpoint, StreamEvent,
    ToolCall,
};
use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::{
    OpenAiAdapterError, ReasoningSummaryPart, ResponsesContent, ResponsesOutput, ResponsesResponse,
    extract_reasoning_stream_text, extract_stream_text, parse_tool_arguments,
    responses_continuation, responses_input_item,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiResponsesConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiResponsesConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self { base_url: base_url.into(), api_key: api_key.into(), model: model.into() }
    }
}

pub struct OpenAiResponsesModel {
    config: OpenAiResponsesConfig,
}

impl OpenAiResponsesModel {
    pub fn new(config: OpenAiResponsesConfig) -> Result<Self, OpenAiAdapterError> {
        if config.base_url.is_empty() || config.api_key.is_empty() || config.model.is_empty() {
            return Err(OpenAiAdapterError::new("配置缺失"));
        }
        Ok(Self { config })
    }

    pub(crate) fn build_request_body(&self, request: &CompletionRequest) -> Value {
        let continuation = responses_continuation(request);
        let input = continuation.as_ref().map(|(_, items)| items.clone()).unwrap_or_else(|| {
            request.conversation.iter().map(responses_input_item).collect::<Vec<_>>()
        });

        let tools = request
            .available_tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                })
            })
            .collect::<Vec<_>>();

        let mut body = json!({
            "model": self.config.model,
            "instructions": request.instructions,
            "input": input,
            "tools": tools,
        });
        if let Some(effort) = &request.model.reasoning_effort {
            body["reasoning"] = json!({"effort": effort, "summary": "auto"});
        }
        if let Some((previous_response_id, _)) = continuation {
            body["previous_response_id"] = json!(previous_response_id);
        }
        body
    }

    pub(crate) fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body
    }

    pub(crate) fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
        let payload: ResponsesResponse = serde_json::from_str(body)
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
        let response_id = payload.id.clone();

        let mut segments = Vec::new();

        for (index, item) in payload.output.into_iter().enumerate() {
            match item {
                ResponsesOutput::Reasoning { summary } => {
                    let text: String = summary
                        .into_iter()
                        .filter_map(|part| match part {
                            ReasoningSummaryPart::SummaryText { text } => Some(text),
                            ReasoningSummaryPart::Other => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        segments.push(CompletionSegment::Thinking(text));
                    }
                }
                ResponsesOutput::Message { content } => {
                    for part in content {
                        if let ResponsesContent::OutputText { text } = part {
                            segments.push(CompletionSegment::Text(text));
                        }
                    }
                }
                ResponsesOutput::FunctionCall { id, call_id, name, arguments } => {
                    let invocation_id =
                        id.or(call_id).unwrap_or_else(|| format!("openai-call-{}", index + 1));
                    segments.push(CompletionSegment::ToolUse({
                        let mut call = ToolCall::new(name)
                            .with_invocation_id(invocation_id)
                            .with_arguments_value(parse_tool_arguments(&arguments)?);
                        if let Some(response_id) = response_id.clone() {
                            call = call.with_response_id(response_id);
                        }
                        call
                    }));
                }
                ResponsesOutput::Other => {}
            }
        }

        Ok(Completion {
            segments,
            checkpoint: payload.id.clone().map(|id| ModelCheckpoint::new("openai-responses", id)),
        })
    }

    pub fn config(&self) -> &OpenAiResponsesConfig {
        &self.config
    }
}

impl LanguageModel for OpenAiResponsesModel {
    type Error = OpenAiAdapterError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let response = Client::new()
            .post(format!("{}/responses", self.config.base_url.trim_end_matches('/')))
            .bearer_auth(&self.config.api_key)
            .json(&self.build_request_body(&request))
            .send()
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        let body = response.text().map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        if !status.is_success() {
            return Err(OpenAiAdapterError::new(format!("请求失败：{status} {body}")));
        }

        self.parse_response_body(&body)
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let response = Client::new()
            .post(format!("{}/responses", self.config.base_url.trim_end_matches('/')))
            .bearer_auth(&self.config.api_key)
            .json(&self.build_streaming_request_body(&request))
            .send()
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body =
                response.text().map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
            return Err(OpenAiAdapterError::new(format!("请求失败：{status} {body}")));
        }

        let reader = io::BufReader::new(response);
        let mut text_buf = String::new();
        let mut thinking_buf = String::new();
        let mut saw_text_delta = false;
        let mut saw_reasoning_delta = false;
        let mut tool_calls: Vec<(String, String, String)> = Vec::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut response_id: Option<String> = None;

        for line in reader.lines() {
            let line = line.map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                break;
            }
            let event: Value = match serde_json::from_str(data) {
                Ok(value) => value,
                Err(_) => continue,
            };
            match event["type"].as_str() {
                Some("response.created") => {
                    response_id = event["response"]["id"].as_str().map(ToString::to_string);
                }
                Some("response.output_text.delta") => {
                    if let Some(delta) = extract_stream_text(&event["delta"]) {
                        saw_text_delta = true;
                        text_buf.push_str(&delta);
                        sink(StreamEvent::TextDelta { text: delta });
                    }
                }
                Some("response.output_text.done") => {
                    if !saw_text_delta {
                        if let Some(text) = extract_stream_text(&event["text"]) {
                            text_buf.push_str(&text);
                            sink(StreamEvent::TextDelta { text });
                        }
                    }
                }
                Some(
                    kind @ ("response.reasoning_summary.delta"
                    | "response.reasoning_summary.done"
                    | "response.reasoning_summary_text.delta"
                    | "response.reasoning_summary_text.done"),
                ) => {
                    if let Some(delta) = extract_reasoning_stream_text(&event) {
                        let is_done_event = kind.ends_with(".done");
                        if !is_done_event || !saw_reasoning_delta {
                            saw_reasoning_delta = saw_reasoning_delta || !is_done_event;
                            thinking_buf.push_str(&delta);
                            sink(StreamEvent::ThinkingDelta { text: delta });
                        }
                    }
                }
                Some("response.function_call_arguments.delta") => {
                    let delta = event["delta"].as_str().unwrap_or("");
                    current_tool_args.push_str(delta);
                }
                Some("response.output_item.added") => {
                    let item = &event["item"];
                    if item["type"].as_str() == Some("function_call") {
                        current_tool_id = item["id"]
                            .as_str()
                            .or_else(|| item["call_id"].as_str())
                            .unwrap_or("")
                            .to_string();
                        current_tool_name = item["name"].as_str().unwrap_or("").to_string();
                        current_tool_args.clear();
                    }
                }
                Some("response.function_call_arguments.done") => {
                    if !current_tool_name.is_empty() {
                        let id = if current_tool_id.is_empty() {
                            format!("openai-stream-call-{}", tool_calls.len() + 1)
                        } else {
                            current_tool_id.clone()
                        };
                        tool_calls.push((id, current_tool_name.clone(), current_tool_args.clone()));
                    }
                    current_tool_id.clear();
                    current_tool_name.clear();
                    current_tool_args.clear();
                }
                Some("response.completed") => {
                    if response_id.is_none() {
                        response_id = event["response"]["id"].as_str().map(ToString::to_string);
                    }
                }
                Some(other) => {
                    sink(StreamEvent::Log { text: format!("[sse] {other}") });
                }
                None => {}
            }
        }

        let mut segments = Vec::new();
        if !thinking_buf.is_empty() {
            segments.push(CompletionSegment::Thinking(thinking_buf));
        }
        if !text_buf.is_empty() {
            segments.push(CompletionSegment::Text(text_buf));
        }
        for (id, name, args) in tool_calls {
            let arguments = parse_tool_arguments(&args).unwrap_or_default();
            segments.push(CompletionSegment::ToolUse({
                let mut call =
                    ToolCall::new(name).with_invocation_id(id).with_arguments_value(arguments);
                if let Some(response_id) = response_id.clone() {
                    call = call.with_response_id(response_id);
                }
                call
            }));
        }
        sink(StreamEvent::Done);
        Ok(Completion {
            segments,
            checkpoint: response_id.map(|id| ModelCheckpoint::new("openai-responses", id)),
        })
    }
}
