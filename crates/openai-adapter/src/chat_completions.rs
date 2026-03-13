use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};

use agent_core::{
    Completion, CompletionRequest, CompletionSegment, LanguageModel, StreamEvent, ToolCall,
};
use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::{
    ChatCompletionsResponse, OpenAiAdapterError, chat_completion_messages, parse_tool_arguments,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiChatCompletionsConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiChatCompletionsConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self { base_url: base_url.into(), api_key: api_key.into(), model: model.into() }
    }
}

pub struct OpenAiChatCompletionsModel {
    config: OpenAiChatCompletionsConfig,
}

#[derive(Default)]
struct StreamingToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl OpenAiChatCompletionsModel {
    pub fn new(config: OpenAiChatCompletionsConfig) -> Result<Self, OpenAiAdapterError> {
        if config.base_url.is_empty() || config.api_key.is_empty() || config.model.is_empty() {
            return Err(OpenAiAdapterError::new("配置缺失"));
        }
        Ok(Self { config })
    }

    pub(crate) fn build_request_body(&self, request: &CompletionRequest) -> Value {
        let mut messages = Vec::new();
        if let Some(instructions) = request.instructions.as_ref().filter(|value| !value.is_empty())
        {
            messages.push(json!({
                "role": "system",
                "content": instructions,
            }));
        }
        messages.extend(chat_completion_messages(&request.conversation));

        let tools = request
            .available_tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect::<Vec<_>>();

        json!({
            "model": self.config.model,
            "messages": messages,
            "tools": tools,
        })
    }

    pub(crate) fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body
    }

    pub(crate) fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
        let payload: ChatCompletionsResponse = serde_json::from_str(body)
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let mut segments = Vec::new();
        for (index, choice) in payload.choices.into_iter().enumerate() {
            if let Some(reasoning) = choice.message.reasoning.filter(|value| !value.is_empty()) {
                segments.push(CompletionSegment::Thinking(reasoning));
            }
            if let Some(content) = choice.message.content.filter(|value| !value.is_empty()) {
                segments.push(CompletionSegment::Text(content));
            }
            for tool_call in choice.message.tool_calls {
                let invocation_id =
                    tool_call.id.unwrap_or_else(|| format!("openai-chat-call-{}", index + 1));
                segments.push(CompletionSegment::ToolUse(
                    ToolCall::new(tool_call.function.name)
                        .with_invocation_id(invocation_id)
                        .with_arguments_value(parse_tool_arguments(&tool_call.function.arguments)?),
                ));
            }
        }

        Ok(Completion { segments, checkpoint: None })
    }

    pub fn config(&self) -> &OpenAiChatCompletionsConfig {
        &self.config
    }
}

impl LanguageModel for OpenAiChatCompletionsModel {
    type Error = OpenAiAdapterError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        if request.model.name != self.config.model {
            return Err(OpenAiAdapterError::new(format!(
                "模型标识不一致：请求为 {}，适配器配置为 {}",
                request.model.name, self.config.model
            )));
        }

        let response = Client::new()
            .post(format!("{}/chat/completions", self.config.base_url.trim_end_matches('/')))
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
            .post(format!("{}/chat/completions", self.config.base_url.trim_end_matches('/')))
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
        let mut tool_calls: BTreeMap<usize, StreamingToolCallState> = BTreeMap::new();

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
            let Some(delta) = event["choices"].get(0).and_then(|choice| choice.get("delta")) else {
                continue;
            };

            if let Some(reasoning) = delta.get("reasoning").and_then(|value| value.as_str()) {
                if !reasoning.is_empty() {
                    thinking_buf.push_str(reasoning);
                    sink(StreamEvent::ThinkingDelta { text: reasoning.to_string() });
                }
            }

            if let Some(content) = delta.get("content").and_then(|value| value.as_str()) {
                if !content.is_empty() {
                    text_buf.push_str(content);
                    sink(StreamEvent::TextDelta { text: content.to_string() });
                }
            }

            if let Some(tool_deltas) = delta.get("tool_calls").and_then(|value| value.as_array()) {
                for tool_delta in tool_deltas {
                    let index = tool_delta
                        .get("index")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(tool_calls.len() as u64)
                        as usize;
                    let state = tool_calls.entry(index).or_default();
                    if let Some(id) = tool_delta.get("id").and_then(|value| value.as_str()) {
                        state.id = Some(id.to_string());
                    }
                    if let Some(name) = tool_delta
                        .get("function")
                        .and_then(|value| value.get("name"))
                        .and_then(|value| value.as_str())
                    {
                        if state.name.is_none() {
                            let invocation_id = state.id.clone().unwrap_or_else(|| {
                                format!("openai-chat-stream-call-{}", index + 1)
                            });
                            let arguments =
                                parse_tool_arguments(&state.arguments).unwrap_or_default();
                            sink(StreamEvent::ToolCallStarted {
                                invocation_id,
                                tool_name: name.to_string(),
                                arguments,
                            });
                        }
                        state.name = Some(name.to_string());
                    }
                    if let Some(arguments_delta) = tool_delta
                        .get("function")
                        .and_then(|value| value.get("arguments"))
                        .and_then(|value| value.as_str())
                    {
                        state.arguments.push_str(arguments_delta);
                    }
                }
            }
        }

        let mut segments = Vec::new();
        if !thinking_buf.is_empty() {
            segments.push(CompletionSegment::Thinking(thinking_buf));
        }
        if !text_buf.is_empty() {
            segments.push(CompletionSegment::Text(text_buf));
        }
        for (index, state) in tool_calls {
            let Some(name) = state.name else {
                continue;
            };
            let invocation_id =
                state.id.unwrap_or_else(|| format!("openai-chat-stream-call-{}", index + 1));
            let arguments = parse_tool_arguments(&state.arguments).unwrap_or_default();
            segments.push(CompletionSegment::ToolUse(
                ToolCall::new(name)
                    .with_invocation_id(invocation_id)
                    .with_arguments_value(arguments),
            ));
        }
        sink(StreamEvent::Done);
        Ok(Completion { segments, checkpoint: None })
    }
}
