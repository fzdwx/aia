use std::{
    collections::BTreeMap,
    fmt,
    io::{self, BufRead},
};

use agent_core::{
    Completion, CompletionRequest, CompletionSegment, ConversationItem, LanguageModel,
    ModelCheckpoint, Role, StreamEvent, ToolCall,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Value, json};

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

    fn build_request_body(&self, request: &CompletionRequest) -> Value {
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
        if let Some(effort) = &request.reasoning_effort {
            body["reasoning"] = json!({"effort": effort, "summary": "auto"});
        }
        if let Some((previous_response_id, _)) = continuation {
            body["previous_response_id"] = json!(previous_response_id);
        }
        body
    }

    fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body
    }

    fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
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

impl OpenAiChatCompletionsModel {
    pub fn new(config: OpenAiChatCompletionsConfig) -> Result<Self, OpenAiAdapterError> {
        if config.base_url.is_empty() || config.api_key.is_empty() || config.model.is_empty() {
            return Err(OpenAiAdapterError::new("配置缺失"));
        }
        Ok(Self { config })
    }

    fn build_request_body(&self, request: &CompletionRequest) -> Value {
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

    fn build_streaming_request_body(&self, request: &CompletionRequest) -> Value {
        let mut body = self.build_request_body(request);
        body["stream"] = json!(true);
        body
    }

    fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
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
        let mut tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args_buf)
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut response_id: Option<String> = None;

        for line in reader.lines() {
            let line = line.map_err(|e| OpenAiAdapterError::new(e.to_string()))?;
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                break;
            }
            let event: Value = match serde_json::from_str(data) {
                Ok(v) => v,
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
                Some(t) if t.contains("reasoning") || t.contains("thinking") => {
                    if let Some(delta) = extract_reasoning_stream_text(&event) {
                        let is_done_event = t.ends_with(".done");
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

fn role_name(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn responses_input_item(item: &ConversationItem) -> Value {
    match item {
        ConversationItem::Message(message) => json!({
            "role": role_name(&message.role),
            "content": message.content,
        }),
        ConversationItem::ToolCall(call) => json!({
            "type": "function_call",
            "call_id": call.invocation_id,
            "name": call.tool_name,
            "arguments": serialize_tool_arguments(&call.arguments),
        }),
        ConversationItem::ToolResult(result) => json!({
            "type": "function_call_output",
            "call_id": result.invocation_id,
            "output": result.content,
        }),
    }
}

fn responses_continuation(request: &CompletionRequest) -> Option<(String, Vec<Value>)> {
    if let Some(checkpoint) = request.resume_checkpoint.as_ref() {
        if checkpoint.protocol == "openai-responses" {
            let input = request
                .conversation
                .iter()
                .filter_map(|item| match item {
                    ConversationItem::ToolResult(_) | ConversationItem::Message(_) => {
                        Some(responses_input_item(item))
                    }
                    ConversationItem::ToolCall(_) => None,
                })
                .collect::<Vec<_>>();
            if !input.is_empty() {
                return Some((checkpoint.token.clone(), input));
            }
        }
    }

    let latest_response_id = request.conversation.iter().rev().find_map(|item| match item {
        ConversationItem::ToolResult(result) => result.response_id.clone(),
        ConversationItem::Message(_) | ConversationItem::ToolCall(_) => None,
    })?;

    let last_user_index = request
        .conversation
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| {
            item.as_message().filter(|message| message.role == Role::User).map(|_| index)
        })
        .unwrap_or(0);

    let input = request
        .conversation
        .iter()
        .skip(last_user_index + 1)
        .filter_map(|item| match item {
            ConversationItem::ToolResult(result)
                if result.response_id.as_deref() == Some(latest_response_id.as_str()) =>
            {
                Some(responses_input_item(item))
            }
            ConversationItem::Message(_)
            | ConversationItem::ToolCall(_)
            | ConversationItem::ToolResult(_) => None,
        })
        .collect::<Vec<_>>();

    if input.is_empty() { None } else { Some((latest_response_id, input)) }
}

fn chat_completion_messages(conversation: &[ConversationItem]) -> Vec<Value> {
    let mut messages = Vec::new();

    for item in conversation {
        match item {
            ConversationItem::Message(message) => {
                messages.push(json!({
                    "role": role_name(&message.role),
                    "content": message.content,
                }));
            }
            ConversationItem::ToolCall(call) => {
                let tool_call = json!({
                    "id": call.invocation_id,
                    "type": "function",
                    "function": {
                        "name": call.tool_name,
                        "arguments": serialize_tool_arguments(&call.arguments),
                    }
                });

                if let Some(last) = messages.last_mut() {
                    let is_assistant =
                        last.get("role").and_then(|value| value.as_str()) == Some("assistant");
                    if is_assistant {
                        if last.get("content").is_none() {
                            last["content"] = json!("");
                        }
                        let mut tool_calls = last
                            .get("tool_calls")
                            .and_then(|value| value.as_array())
                            .cloned()
                            .unwrap_or_default();
                        tool_calls.push(tool_call);
                        last["tool_calls"] = Value::Array(tool_calls);
                        continue;
                    }
                }

                messages.push(json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [tool_call],
                }));
            }
            ConversationItem::ToolResult(result) => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": result.invocation_id,
                    "content": result.content,
                }));
            }
        }
    }

    messages
}

fn serialize_tool_arguments(arguments: &Value) -> String {
    serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())
}

fn parse_tool_arguments(arguments: &str) -> Result<Value, OpenAiAdapterError> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
    if !value.is_object() {
        return Err(OpenAiAdapterError::new("工具参数必须是对象"));
    }
    Ok(value)
}

fn extract_stream_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(values) => {
            let text = values.iter().filter_map(extract_stream_text).collect::<Vec<_>>().join("");
            if text.is_empty() { None } else { Some(text) }
        }
        Value::Object(map) => {
            for key in ["text", "summary_text", "content", "value"] {
                if let Some(text) = map.get(key).and_then(extract_stream_text) {
                    return Some(text);
                }
            }
            let text = map.values().filter_map(extract_stream_text).collect::<Vec<_>>().join("");
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

fn extract_reasoning_stream_text(event: &Value) -> Option<String> {
    extract_stream_text(&event["delta"])
        .or_else(|| extract_stream_text(&event["text"]))
        .or_else(|| extract_stream_text(&event["part"]["text"]))
}

#[derive(Default)]
struct StreamingToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Deserialize)]
struct ResponsesResponse {
    #[serde(default)]
    id: Option<String>,
    output: Vec<ResponsesOutput>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ResponsesOutput {
    #[serde(rename = "message")]
    Message { content: Vec<ResponsesContent> },
    #[serde(rename = "function_call")]
    FunctionCall { id: Option<String>, call_id: Option<String>, name: String, arguments: String },
    #[serde(rename = "reasoning")]
    Reasoning {
        #[serde(default)]
        summary: Vec<ReasoningSummaryPart>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ResponsesContent {
    #[serde(rename = "output_text")]
    OutputText { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ReasoningSummaryPart {
    #[serde(rename = "summary_text")]
    SummaryText { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Deserialize)]
struct ChatCompletionMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatCompletionToolCall>,
}

#[derive(Deserialize)]
struct ChatCompletionToolCall {
    id: Option<String>,
    function: ChatCompletionFunction,
}

#[derive(Deserialize)]
struct ChatCompletionFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OpenAiAdapterError {
    message: String,
}

impl OpenAiAdapterError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for OpenAiAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for OpenAiAdapterError {}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use agent_core::{
        CompletionRequest, CompletionSegment, ConversationItem, LanguageModel, Message,
        ModelCheckpoint, ModelDisposition, ModelIdentity, Role, ToolCall, ToolDefinition,
        ToolResult,
    };
    use serde_json::json;

    use super::{
        OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel, OpenAiResponsesConfig,
        OpenAiResponsesModel,
    };

    fn sample_request() -> CompletionRequest {
        CompletionRequest {
            model: ModelIdentity::new("openai", "gpt-4.1-mini", ModelDisposition::Balanced),
            instructions: Some("保持简洁".into()),
            conversation: vec![
                ConversationItem::Message(Message::new(Role::System, "你是代码助手")),
                ConversationItem::Message(Message::new(Role::User, "帮我总结当前工作区")),
            ],
            resume_checkpoint: None,
            available_tools: vec![ToolDefinition::new("search_code", "搜索代码").with_parameter(
                "query",
                "关键字",
                true,
            )],
            reasoning_effort: None,
        }
    }

    #[test]
    fn 请求体会映射模型指令消息与工具() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let body = model.build_request_body(&sample_request());

        assert_eq!(body["model"], json!("gpt-4.1-mini"));
        assert_eq!(body["instructions"], json!("保持简洁"));
        assert_eq!(body["input"][0]["role"], json!("system"));
        assert_eq!(body["input"][1]["content"], json!("帮我总结当前工作区"));
        assert_eq!(body["tools"][0]["name"], json!("search_code"));
        assert_eq!(body["tools"][0]["parameters"]["required"], json!(["query"]));
        // reasoning block not sent when reasoning_effort is None
        assert!(body.get("reasoning").is_none() || body["reasoning"].is_null());
    }

    #[test]
    fn 请求体带_reasoning_effort_时发送_reasoning_块() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let mut request = sample_request();
        request.reasoning_effort = Some("high".into());
        let body = model.build_request_body(&request);

        assert_eq!(body["reasoning"]["effort"], json!("high"));
        assert_eq!(body["reasoning"]["summary"], json!("auto"));
    }

    #[test]
    fn responses_请求体会保留结构化工具调用与结果() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");
        let call = ToolCall::new("search_code")
            .with_invocation_id("call_1")
            .with_argument("query", "agent-runtime");
        let mut request = sample_request();
        request.conversation.push(ConversationItem::ToolCall(call.clone()));
        request
            .conversation
            .push(ConversationItem::ToolResult(ToolResult::from_call(&call, "found")));

        let body = model.build_request_body(&request);

        assert_eq!(body["input"][2]["type"], json!("function_call"));
        assert_eq!(body["input"][2]["call_id"], json!("call_1"));
        assert_eq!(body["input"][3]["type"], json!("function_call_output"));
        assert_eq!(body["input"][3]["call_id"], json!("call_1"));
        assert_eq!(body["input"][3]["output"], json!("found"));
    }

    #[test]
    fn responses_工具续调会带_previous_response_id_且只发送工具结果() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");
        let call = ToolCall::new("search_code")
            .with_invocation_id("call_1")
            .with_response_id("resp_123")
            .with_argument("query", "agent-runtime");
        let mut request = sample_request();
        request.conversation.push(ConversationItem::ToolCall(call.clone()));
        request
            .conversation
            .push(ConversationItem::ToolResult(ToolResult::from_call(&call, "found")));

        let body = model.build_request_body(&request);

        assert_eq!(body["previous_response_id"], json!("resp_123"));
        assert_eq!(body["input"].as_array().map(|items| items.len()), Some(1));
        assert_eq!(body["input"][0]["type"], json!("function_call_output"));
        assert_eq!(body["input"][0]["call_id"], json!("call_1"));
    }

    #[test]
    fn responses_新一轮用户输入会沿用_previous_response_id() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");
        let mut request = sample_request();
        request.resume_checkpoint = Some(ModelCheckpoint::new("openai-responses", "resp_123"));
        request.conversation =
            vec![ConversationItem::Message(Message::new(Role::User, "第二轮问题"))];

        let body = model.build_request_body(&request);

        assert_eq!(body["previous_response_id"], json!("resp_123"));
        assert_eq!(body["input"].as_array().map(|items| items.len()), Some(1));
        assert_eq!(body["input"][0]["role"], json!("user"));
        assert_eq!(body["input"][0]["content"], json!("第二轮问题"));
    }

    #[test]
    fn 响应体可解析文本与工具调用() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let completion = model
            .parse_response_body(
                r#"{
                    "id": "resp_123",
                    "output": [
                        {
                            "type": "message",
                            "role": "assistant",
                            "content": [
                                {"type": "output_text", "text": "第一段"},
                                {"type": "output_text", "text": "第二段"}
                            ]
                        },
                        {
                            "type": "function_call",
                            "name": "search_code",
                            "arguments": "{\"query\":\"agent-runtime\"}"
                        }
                    ]
                }"#,
            )
            .expect("响应解析成功");

        assert_eq!(completion.plain_text(), "第一段\n第二段");
        assert!(completion.segments.iter().any(|segment| matches!(
            segment,
            agent_core::CompletionSegment::ToolUse(ToolCall { tool_name, response_id, .. })
                if tool_name == "search_code" && response_id.as_deref() == Some("resp_123")
        )));
    }

    #[test]
    fn 响应体可解析推理摘要() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "o4-mini",
        ))
        .expect("模型创建成功");

        let completion = model
            .parse_response_body(
                r#"{
                    "output": [
                        {
                            "type": "reasoning",
                            "id": "rs_1",
                            "summary": [
                                {"type": "summary_text", "text": "我先分析需求"},
                                {"type": "summary_text", "text": "，然后给出方案"}
                            ]
                        },
                        {
                            "type": "message",
                            "role": "assistant",
                            "content": [
                                {"type": "output_text", "text": "这是回答"}
                            ]
                        }
                    ]
                }"#,
            )
            .expect("响应解析成功");

        assert_eq!(completion.thinking_text(), Some("我先分析需求，然后给出方案".into()));
        assert_eq!(completion.plain_text(), "这是回答");
    }

    #[test]
    fn 可通过本地假服务完成一次真实调用() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let body = r#"{"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"来自假服务"}]}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );

            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            format!("http://{address}"),
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let completion = model.complete(sample_request()).expect("调用成功");

        handle.join().expect("服务线程退出");
        assert_eq!(completion.plain_text(), "来自假服务");
    }

    #[test]
    fn 请求里的模型标识与适配器配置不一致时会报错() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let mut request = sample_request();
        request.model.name = "gpt-4.1".into();

        let error = model.complete(request).expect_err("应当因为模型不一致而失败");

        assert!(error.to_string().contains("模型标识不一致"));
    }

    #[test]
    fn 缺少提供商调用标识时会生成唯一替代标识() {
        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let completion = model
            .parse_response_body(
                r#"{
                    "output": [
                        {"type": "function_call", "name": "search_code", "arguments": "{\"query\":\"a\"}"},
                        {"type": "function_call", "name": "search_code", "arguments": "{\"query\":\"b\"}"}
                    ]
                }"#,
            )
            .expect("响应解析成功");

        let mut ids: Vec<String> = completion
            .segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::ToolUse(call) => Some(call.invocation_id.clone()),
                CompletionSegment::Text(_) | CompletionSegment::Thinking(_) => None,
            })
            .collect::<Vec<_>>();

        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 2);
        assert!(ids[0].starts_with("openai-call-"));
        assert!(ids[1].starts_with("openai-call-"));
    }

    #[test]
    fn 流式调用可逐段收到文本与思考() {
        use agent_core::StreamEvent;

        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let sse_body = [
                r#"data: {"type":"response.reasoning_summary_text.delta","delta":"思考中"}"#,
                r#"data: {"type":"response.output_text.delta","delta":"你"}"#,
                r#"data: {"type":"response.output_text.delta","delta":"好"}"#,
                r#"data: [DONE]"#,
            ]
            .join("\n\n");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{sse_body}\n\n"
            );
            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            format!("http://{address}"),
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let mut deltas = Vec::new();
        let completion = model
            .complete_streaming(sample_request(), &mut |event| {
                deltas.push(event);
            })
            .expect("流式调用成功");

        handle.join().expect("服务线程退出");
        assert_eq!(completion.plain_text(), "你好");
        assert_eq!(completion.thinking_text(), Some("思考中".into()));
        assert_eq!(
            deltas,
            vec![
                StreamEvent::ThinkingDelta { text: "思考中".into() },
                StreamEvent::TextDelta { text: "你".into() },
                StreamEvent::TextDelta { text: "好".into() },
                StreamEvent::Done,
            ]
        );
    }

    #[test]
    fn 流式调用可解析对象形态的推理摘要增量() {
        use agent_core::StreamEvent;

        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let sse_body = [
                r#"data: {"type":"response.reasoning_summary.delta","delta":{"text":"先分析"}}"#,
                r#"data: {"type":"response.output_text.delta","delta":"答案"}"#,
                r#"data: [DONE]"#,
            ]
            .join("\n\n");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{sse_body}\n\n"
            );
            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            format!("http://{address}"),
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let mut deltas = Vec::new();
        let completion = model
            .complete_streaming(sample_request(), &mut |event| {
                deltas.push(event);
            })
            .expect("流式调用成功");

        handle.join().expect("服务线程退出");
        assert_eq!(completion.thinking_text(), Some("先分析".into()));
        assert_eq!(completion.plain_text(), "答案");
        assert_eq!(
            deltas,
            vec![
                StreamEvent::ThinkingDelta { text: "先分析".into() },
                StreamEvent::TextDelta { text: "答案".into() },
                StreamEvent::Done,
            ]
        );
    }

    #[test]
    fn 流式调用可解析_done_事件里的推理摘要文本() {
        use agent_core::StreamEvent;

        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let sse_body = [
                r#"data: {"type":"response.reasoning_summary_text.done","text":"先分析"}"#,
                r#"data: {"type":"response.output_text.done","text":"答案"}"#,
                r#"data: [DONE]"#,
            ]
            .join("\n\n");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{sse_body}\n\n"
            );
            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            format!("http://{address}"),
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let mut deltas = Vec::new();
        let completion = model
            .complete_streaming(sample_request(), &mut |event| {
                deltas.push(event);
            })
            .expect("流式调用成功");

        handle.join().expect("服务线程退出");
        assert_eq!(completion.thinking_text(), Some("先分析".into()));
        assert_eq!(completion.plain_text(), "答案");
        assert_eq!(
            deltas,
            vec![
                StreamEvent::ThinkingDelta { text: "先分析".into() },
                StreamEvent::TextDelta { text: "答案".into() },
                StreamEvent::Done,
            ]
        );
    }

    #[test]
    fn responses_流式工具调用会继承_response_id() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let sse_body = [
                r#"data: {"type":"response.created","response":{"id":"resp_123"}}"#,
                r#"data: {"type":"response.output_item.added","item":{"type":"function_call","id":"call_1","name":"search_code"}}"#,
                r#"data: {"type":"response.function_call_arguments.delta","delta":"{\"query\":\"agent-runtime\"}"}"#,
                r#"data: {"type":"response.function_call_arguments.done"}"#,
                r#"data: [DONE]"#,
            ]
            .join("\n\n");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{sse_body}\n\n"
            );
            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
            format!("http://{address}"),
            "test-key",
            "gpt-4.1-mini",
        ))
        .expect("模型创建成功");

        let completion =
            model.complete_streaming(sample_request(), &mut |_| {}).expect("流式调用成功");

        handle.join().expect("服务线程退出");
        assert!(completion.segments.iter().any(|segment| matches!(
            segment,
            CompletionSegment::ToolUse(ToolCall { tool_name, invocation_id, response_id, .. })
                if tool_name == "search_code"
                    && invocation_id == "call_1"
                    && response_id.as_deref() == Some("resp_123")
        )));
    }

    #[test]
    fn 聊天补全请求体会映射_messages_与工具() {
        let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "minum-security-llm",
        ))
        .expect("模型创建成功");

        let body = model.build_request_body(&sample_request());

        assert_eq!(body["model"], json!("minum-security-llm"));
        assert_eq!(body["messages"][0]["role"], json!("system"));
        assert_eq!(body["messages"][0]["content"], json!("保持简洁"));
        assert_eq!(body["messages"][1]["content"], json!("你是代码助手"));
        assert_eq!(body["messages"][2]["content"], json!("帮我总结当前工作区"));
        assert_eq!(body["tools"][0]["function"]["name"], json!("search_code"));
    }

    #[test]
    fn 聊天补全请求体会保留结构化工具链路() {
        let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "minum-security-llm",
        ))
        .expect("模型创建成功");
        let call = ToolCall::new("search_code")
            .with_invocation_id("call_1")
            .with_argument("query", "agent-runtime");
        let mut request = sample_request();
        request.model.name = "minum-security-llm".into();
        request.conversation.push(ConversationItem::ToolCall(call.clone()));
        request
            .conversation
            .push(ConversationItem::ToolResult(ToolResult::from_call(&call, "found")));

        let body = model.build_request_body(&request);

        assert_eq!(body["messages"][3]["role"], json!("assistant"));
        assert_eq!(body["messages"][3]["tool_calls"][0]["id"], json!("call_1"));
        assert_eq!(body["messages"][3]["tool_calls"][0]["function"]["name"], json!("search_code"));
        assert_eq!(body["messages"][4]["role"], json!("tool"));
        assert_eq!(body["messages"][4]["tool_call_id"], json!("call_1"));
        assert_eq!(body["messages"][4]["content"], json!("found"));
    }

    #[test]
    fn 聊天补全响应体可解析文本与工具调用() {
        let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
            "http://127.0.0.1:1",
            "test-key",
            "minum-security-llm",
        ))
        .expect("模型创建成功");

        let completion = model
            .parse_response_body(
                r#"{
                    "choices": [
                        {
                            "message": {
                                "role": "assistant",
                                "content": "第一段\n第二段",
                                "tool_calls": [
                                    {
                                        "id": "call_1",
                                        "type": "function",
                                        "function": {
                                            "name": "search_code",
                                            "arguments": "{\"query\":\"agent-runtime\"}"
                                        }
                                    }
                                ]
                            }
                        }
                    ]
                }"#,
            )
            .expect("响应解析成功");

        assert_eq!(completion.plain_text(), "第一段\n第二段");
        assert!(completion.segments.iter().any(|segment| matches!(
            segment,
            agent_core::CompletionSegment::ToolUse(ToolCall { tool_name, invocation_id, .. })
                if tool_name == "search_code" && invocation_id == "call_1"
        )));
    }

    #[test]
    fn 聊天补全可通过本地假服务完成一次真实调用() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let body =
                r#"{"choices":[{"message":{"role":"assistant","content":"来自聊天补全假服务"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );

            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
            format!("http://{address}"),
            "test-key",
            "minum-security-llm",
        ))
        .expect("模型创建成功");

        let mut request = sample_request();
        request.model.name = "minum-security-llm".into();
        let completion = model.complete(request).expect("调用成功");

        handle.join().expect("服务线程退出");
        assert_eq!(completion.plain_text(), "来自聊天补全假服务");
    }

    #[test]
    fn 聊天补全流式调用可逐段收到文本与工具() {
        use agent_core::StreamEvent;

        let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
        let address = listener.local_addr().expect("读取地址成功");

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("读取请求成功");

            let sse_body = [
                r#"data: {"choices":[{"delta":{"reasoning":"先分析"}}]}"#,
                r#"data: {"choices":[{"delta":{"content":"答"}}]}"#,
                r#"data: {"choices":[{"delta":{"content":"案"}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"search_code","arguments":"{\"query\":\"agent-runtime"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"}"}}]}}]}"#,
                r#"data: [DONE]"#,
            ]
            .join("\n\n");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{sse_body}\n\n"
            );
            stream.write_all(response.as_bytes()).expect("写回响应成功");
        });

        let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
            format!("http://{address}"),
            "test-key",
            "minum-security-llm",
        ))
        .expect("模型创建成功");

        let mut request = sample_request();
        request.model.name = "minum-security-llm".into();
        let mut deltas = Vec::new();
        let completion = model
            .complete_streaming(request, &mut |event| deltas.push(event))
            .expect("流式调用成功");

        handle.join().expect("服务线程退出");
        assert_eq!(completion.thinking_text(), Some("先分析".into()));
        assert_eq!(completion.plain_text(), "答案");
        assert!(completion.segments.iter().any(|segment| matches!(
            segment,
            CompletionSegment::ToolUse(ToolCall { tool_name, invocation_id, .. })
                if tool_name == "search_code" && invocation_id == "call_1"
        )));
        assert_eq!(
            deltas,
            vec![
                StreamEvent::ThinkingDelta { text: "先分析".into() },
                StreamEvent::TextDelta { text: "答".into() },
                StreamEvent::TextDelta { text: "案".into() },
                StreamEvent::Done,
            ]
        );
    }
}
