use std::fmt;

use agent_core::{Completion, CompletionRequest, CompletionSegment, LanguageModel, Role, ToolCall};
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
        let input = request
            .conversation
            .iter()
            .map(|message| {
                json!({
                    "role": role_name(&message.role),
                    "content": message.content,
                })
            })
            .collect::<Vec<_>>();

        let tools = request
            .available_tools
            .iter()
            .map(|tool| {
                let properties = tool
                    .parameters
                    .iter()
                    .map(|parameter| {
                        (
                            parameter.name.clone(),
                            json!({
                                "type": "string",
                                "description": parameter.description,
                            }),
                        )
                    })
                    .collect::<serde_json::Map<String, Value>>();

                let required = tool
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.required)
                    .map(|parameter| parameter.name.clone())
                    .collect::<Vec<_>>();

                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                        "additionalProperties": false,
                    }
                })
            })
            .collect::<Vec<_>>();

        json!({
            "model": self.config.model,
            "instructions": request.instructions,
            "input": input,
            "tools": tools,
        })
    }

    fn parse_response_body(&self, body: &str) -> Result<Completion, OpenAiAdapterError> {
        let payload: ResponsesResponse = serde_json::from_str(body)
            .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;

        let mut segments = Vec::new();

        for (index, item) in payload.output.into_iter().enumerate() {
            match item {
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
                    segments.push(CompletionSegment::ToolUse(
                        ToolCall::new(name)
                            .with_invocation_id(invocation_id)
                            .with_arguments(parse_tool_arguments(&arguments)?),
                    ));
                }
                ResponsesOutput::Other => {}
            }
        }

        Ok(Completion { segments })
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
}

fn role_name(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn parse_tool_arguments(arguments: &str) -> Result<Vec<(String, String)>, OpenAiAdapterError> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|error| OpenAiAdapterError::new(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(OpenAiAdapterError::new("工具参数必须是对象"));
    };

    Ok(object
        .iter()
        .map(|(key, value)| {
            let text = match value {
                Value::String(inner) => inner.clone(),
                other => other.to_string(),
            };
            (key.clone(), text)
        })
        .collect())
}

#[derive(Deserialize)]
struct ResponsesResponse {
    output: Vec<ResponsesOutput>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ResponsesOutput {
    #[serde(rename = "message")]
    Message { content: Vec<ResponsesContent> },
    #[serde(rename = "function_call")]
    FunctionCall { id: Option<String>, call_id: Option<String>, name: String, arguments: String },
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
        CompletionRequest, CompletionSegment, LanguageModel, Message, ModelDisposition,
        ModelIdentity, Role, ToolCall, ToolDefinition,
    };
    use serde_json::json;

    use super::{OpenAiResponsesConfig, OpenAiResponsesModel};

    fn sample_request() -> CompletionRequest {
        CompletionRequest {
            model: ModelIdentity::new("openai", "gpt-4.1-mini", ModelDisposition::Balanced),
            instructions: Some("保持简洁".into()),
            conversation: vec![
                Message::new(Role::System, "你是代码助手"),
                Message::new(Role::User, "帮我总结当前工作区"),
            ],
            available_tools: vec![ToolDefinition::new("search_code", "搜索代码").with_parameter(
                "query",
                "关键字",
                true,
            )],
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
            agent_core::CompletionSegment::ToolUse(ToolCall { tool_name, .. }) if tool_name == "search_code"
        )));
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
                CompletionSegment::Text(_) => None,
            })
            .collect::<Vec<_>>();

        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 2);
        assert!(ids[0].starts_with("openai-call-"));
        assert!(ids[1].starts_with("openai-call-"));
    }
}
