use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use agent_core::{
    CompletionRequest, CompletionSegment, ConversationItem, LanguageModel, Message,
    ModelCheckpoint, ModelDisposition, ModelIdentity, Role, StreamEvent, ToolCall, ToolDefinition,
    ToolResult,
};
use serde_json::json;

use crate::{
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
    request.model.reasoning_effort = Some("high".into());
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
    request.conversation.push(ConversationItem::ToolResult(ToolResult::from_call(&call, "found")));

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
    request.conversation.push(ConversationItem::ToolResult(ToolResult::from_call(&call, "found")));

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
    request.conversation = vec![ConversationItem::Message(Message::new(Role::User, "第二轮问题"))];

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
            agent_core::StreamEvent::ThinkingDelta { text: "先分析".into() },
            agent_core::StreamEvent::TextDelta { text: "答案".into() },
            agent_core::StreamEvent::Done,
        ]
    );
}

#[test]
fn 流式调用可解析_done_事件里的推理摘要文本() {
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

    let completion = model.complete_streaming(sample_request(), &mut |_| {}).expect("流式调用成功");

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

    let mut request = sample_request();
    request.model.name = "minum-security-llm".into();
    let body = model.build_request_body(&request);

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
    request.conversation.push(ConversationItem::ToolResult(ToolResult::from_call(&call, "found")));

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
    let completion =
        model.complete_streaming(request, &mut |event| deltas.push(event)).expect("流式调用成功");

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
