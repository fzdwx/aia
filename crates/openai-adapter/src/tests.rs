use std::{
    future::Future,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use agent_core::{
    AbortSignal, CompletionRequest, CompletionSegment, CompletionStopReason, ConversationItem,
    LanguageModel, Message, ModelDisposition, ModelIdentity, PromptCacheConfig,
    PromptCacheRetention, Role, StreamEvent, ToolCall, ToolDefinition, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
        max_output_tokens: None,
        available_tools: vec![ToolDefinition::new("search_code", "搜索代码").with_parameter(
            "query",
            "关键字",
            true,
        )],
        parallel_tool_calls: Some(true),
        prompt_cache: None,
        user_agent: None,
        timeout: None,
        trace_context: None,
    }
}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct SearchToolArgs {
    #[tool_schema(description = "要搜索的关键字")]
    query: String,
}

fn run_async<T>(future: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("测试运行时创建成功");
    runtime.block_on(future)
}

fn complete_model<M: LanguageModel>(
    model: &M,
    request: CompletionRequest,
) -> Result<agent_core::Completion, M::Error> {
    run_async(model.complete_streaming(request, &AbortSignal::new(), &mut |_| {}))
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
    assert_eq!(body["parallel_tool_calls"], json!(true));
    assert!(body.get("reasoning").is_none() || body["reasoning"].is_null());
}

#[test]
fn responses_请求体会透传自研_schema_工具参数且不包含_schema_元字段() {
    let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "gpt-4.1-mini",
    ))
    .expect("模型创建成功");

    let mut request = sample_request();
    request.available_tools = vec![
        ToolDefinition::new("search_code", "搜索代码").with_parameters_schema::<SearchToolArgs>(),
    ];

    let body = model.build_request_body(&request);

    assert!(body["tools"][0]["parameters"].get("$schema").is_none());
    assert_eq!(body["tools"][0]["parameters"]["properties"]["query"]["type"], "string");
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
fn responses_请求体会映射_output_limit() {
    let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "gpt-4.1-mini",
    ))
    .expect("模型创建成功");

    let mut request = sample_request();
    request.max_output_tokens = Some(131_072);
    let body = model.build_request_body(&request);

    assert_eq!(body["max_output_tokens"], json!(131_072));
}

#[test]
fn responses_请求体会映射_prompt_cache() {
    let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "gpt-4.1-mini",
    ))
    .expect("模型创建成功");

    let mut request = sample_request();
    request.prompt_cache = Some(PromptCacheConfig {
        key: Some("aia:test-session".into()),
        retention: Some(PromptCacheRetention::OneDay),
    });
    let body = model.build_request_body(&request);

    assert_eq!(body["prompt_cache_key"], json!("aia:test-session"));
    assert_eq!(body["prompt_cache_retention"], json!("24h"));
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
fn responses_工具结果请求体会发送完整上下文() {
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

    assert!(body.get("previous_response_id").is_none());
    assert_eq!(body["input"].as_array().map(|items| items.len()), Some(4));
    assert_eq!(body["input"][2]["type"], json!("function_call"));
    assert_eq!(body["input"][2]["call_id"], json!("call_1"));
    assert_eq!(body["input"][3]["type"], json!("function_call_output"));
    assert_eq!(body["input"][3]["call_id"], json!("call_1"));
}

#[test]
fn chat_completions_请求体会映射_parallel_tool_calls() {
    let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "gpt-4.1-mini",
    ))
    .expect("模型创建成功");

    let body = model.build_request_body(&sample_request());

    assert_eq!(body["parallel_tool_calls"], json!(true));
    assert_eq!(body["tools"][0]["function"]["name"], json!("search_code"));
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
                    "usage": {
                        "input_tokens": 21,
                        "output_tokens": 9,
                        "total_tokens": 30
                    },
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
    assert_eq!(completion.stop_reason, CompletionStopReason::ToolUse);
    assert_eq!(
        completion.response_body.as_deref(),
        Some(
            r#"{
                    "usage": {
                        "input_tokens": 21,
                        "output_tokens": 9,
                        "total_tokens": 30
                    },
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
                }"#
        )
    );
    assert_eq!(completion.usage.as_ref().map(|usage| usage.input_tokens), Some(21));
    assert_eq!(completion.usage.as_ref().map(|usage| usage.output_tokens), Some(9));
    assert_eq!(completion.usage.as_ref().map(|usage| usage.total_tokens), Some(30));
    assert_eq!(completion.usage.as_ref().map(|usage| usage.cached_tokens), Some(0));
    assert!(completion.segments.iter().any(|segment| matches!(
        segment,
        agent_core::CompletionSegment::ToolUse(ToolCall { tool_name, response_id, .. })
            if tool_name == "search_code" && response_id.as_deref() == Some("resp_123")
    )));
}

#[test]
fn responses_响应体可解析_cached_tokens() {
    let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "gpt-4.1-mini",
    ))
    .expect("模型创建成功");

    let completion = model
        .parse_response_body(
            r#"{
                    "usage": {
                        "input_tokens": 120,
                        "output_tokens": 12,
                        "total_tokens": 132,
                        "input_tokens_details": {
                            "cached_tokens": 96
                        }
                    },
                    "output": [
                        {
                            "type": "message",
                            "role": "assistant",
                            "content": [
                                {"type": "output_text", "text": "命中缓存"}
                            ]
                        }
                    ]
                }"#,
        )
        .expect("响应解析成功");

    assert_eq!(completion.usage.as_ref().map(|usage| usage.cached_tokens), Some(96));
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
    assert_eq!(completion.stop_reason, CompletionStopReason::Stop);
}

#[test]
fn 可通过本地假服务完成一次真实调用() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("读取请求成功");

        let body = [
            r#"data: {"type":"response.output_text.delta","delta":"来自假服务"}"#,
            r#"data: [DONE]"#,
        ]
        .join("\n\n");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}\n\n",
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

    let completion = complete_model(&model, sample_request()).expect("调用成功");

    handle.join().expect("服务线程退出");
    assert_eq!(completion.plain_text(), "来自假服务");
    assert_eq!(completion.stop_reason, CompletionStopReason::Stop);
}

#[test]
fn 真实调用会透传_user_agent_请求头() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let size = stream.read(&mut buffer).expect("读取请求成功");
        let request_text = String::from_utf8_lossy(&buffer[..size]).to_lowercase();
        assert!(request_text.contains("user-agent: aia-test/1.0\r\n"));

        let body = [
            r#"data: {"type":"response.output_text.delta","delta":"来自假服务"}"#,
            r#"data: [DONE]"#,
        ]
        .join("\n\n");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}\n\n",
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

    let mut request = sample_request();
    request.user_agent = Some("aia-test/1.0".into());
    let completion = complete_model(&model, request).expect("调用成功");

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

    let error = complete_model(&model, request).expect_err("应当因为模型不一致而失败");

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
    let completion =
        run_async(model.complete_streaming(sample_request(), &AbortSignal::new(), &mut |event| {
            deltas.push(event);
        }))
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
fn responses_流式失败消息包含请求路径() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("读取请求成功");

        let body = r#"{"error":"upstream failed"}"#;
        let response = format!(
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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

    let error =
        run_async(model.complete_streaming(sample_request(), &AbortSignal::new(), &mut |_| {}))
            .expect_err("流式调用应失败");

    handle.join().expect("服务线程退出");
    assert!(error.to_string().contains("/responses"));
}

#[test]
fn responses_流式调用在_abort_后返回取消错误() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("读取请求成功");

        let response =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        stream.write_all(response.as_bytes()).expect("写回响应头成功");
        stream.flush().expect("刷新响应头成功");
        thread::sleep(std::time::Duration::from_millis(120));
        let _ = stream.write_all(b"data: [DONE]\n\n");
    });

    let model = OpenAiResponsesModel::new(OpenAiResponsesConfig::new(
        format!("http://{address}"),
        "test-key",
        "gpt-4.1-mini",
    ))
    .expect("模型创建成功");

    let abort = AbortSignal::new();
    let cancel = abort.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(30));
        cancel.abort();
    });

    let error = run_async(model.complete_streaming(sample_request(), &abort, &mut |_| {}))
        .expect_err("应当因取消而失败");

    handle.join().expect("服务线程退出");
    assert!(error.is_cancelled());
}

#[test]
fn chat_completions_流式调用在_abort_后返回取消错误() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("读取请求成功");

        let response =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        stream.write_all(response.as_bytes()).expect("写回响应头成功");
        stream.flush().expect("刷新响应头成功");
        thread::sleep(std::time::Duration::from_millis(120));
        let _ = stream.write_all(b"data: [DONE]\n\n");
    });

    let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
        format!("http://{address}"),
        "test-key",
        "minum-security-llm",
    ))
    .expect("模型创建成功");

    let mut request = sample_request();
    request.model.name = "minum-security-llm".into();
    let abort = AbortSignal::new();
    let cancel = abort.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(30));
        cancel.abort();
    });

    let error = run_async(model.complete_streaming(request, &abort, &mut |_| {}))
        .expect_err("应当因取消而失败");

    handle.join().expect("服务线程退出");
    assert!(error.is_cancelled());
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
    let completion =
        run_async(model.complete_streaming(sample_request(), &AbortSignal::new(), &mut |event| {
            deltas.push(event);
        }))
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
    let completion =
        run_async(model.complete_streaming(sample_request(), &AbortSignal::new(), &mut |event| {
            deltas.push(event);
        }))
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
        run_async(model.complete_streaming(sample_request(), &AbortSignal::new(), &mut |_| {}))
            .expect("流式调用成功");

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
fn 未知_reasoning_事件不会被当成思考内容() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("读取请求成功");

        let sse_body = [
            r#"data: {"type":"response.reasoning_text.delta","delta":{"content":"这段文本不应进入 Thought"}}"#,
            r#"data: {"type":"response.output_text.done","text":"真正的最终回答"}"#,
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
    let completion =
        run_async(model.complete_streaming(sample_request(), &AbortSignal::new(), &mut |event| {
            deltas.push(event)
        }))
        .expect("流式调用成功");

    handle.join().expect("服务线程退出");
    assert_eq!(completion.thinking_text(), None);
    assert_eq!(completion.plain_text(), "真正的最终回答");
    assert_eq!(
        deltas,
        vec![
            StreamEvent::Log { text: "[sse] response.reasoning_text.delta".into() },
            StreamEvent::TextDelta { text: "真正的最终回答".into() },
            StreamEvent::Done,
        ]
    );
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
fn 聊天补全请求体会映射_output_limit() {
    let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "minum-security-llm",
    ))
    .expect("模型创建成功");

    let mut request = sample_request();
    request.model.name = "minum-security-llm".into();
    request.max_output_tokens = Some(131_072);
    let body = model.build_request_body(&request);

    assert_eq!(body["max_completion_tokens"], json!(131_072));
}

#[test]
fn 聊天补全请求体会映射_prompt_cache() {
    let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "minum-security-llm",
    ))
    .expect("模型创建成功");

    let mut request = sample_request();
    request.model.name = "minum-security-llm".into();
    request.prompt_cache = Some(PromptCacheConfig {
        key: Some("aia:test-session".into()),
        retention: Some(PromptCacheRetention::OneDay),
    });
    let body = model.build_request_body(&request);

    assert_eq!(body["prompt_cache_key"], json!("aia:test-session"));
    assert_eq!(body["prompt_cache_retention"], json!("24h"));
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
                    "usage": {
                        "prompt_tokens": 18,
                        "completion_tokens": 7,
                        "total_tokens": 25
                    },
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
    assert_eq!(completion.stop_reason, CompletionStopReason::ToolUse);
    assert_eq!(
        completion.response_body.as_deref(),
        Some(
            r#"{
                    "usage": {
                        "prompt_tokens": 18,
                        "completion_tokens": 7,
                        "total_tokens": 25
                    },
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
                }"#
        )
    );
    assert_eq!(completion.usage.as_ref().map(|usage| usage.input_tokens), Some(18));
    assert_eq!(completion.usage.as_ref().map(|usage| usage.output_tokens), Some(7));
    assert_eq!(completion.usage.as_ref().map(|usage| usage.total_tokens), Some(25));
    assert_eq!(completion.usage.as_ref().map(|usage| usage.cached_tokens), Some(0));
    assert!(completion.segments.iter().any(|segment| matches!(
        segment,
        agent_core::CompletionSegment::ToolUse(ToolCall { tool_name, invocation_id, .. })
            if tool_name == "search_code" && invocation_id == "call_1"
    )));
}

#[test]
fn 聊天补全响应体可解析_cached_tokens() {
    let model = OpenAiChatCompletionsModel::new(OpenAiChatCompletionsConfig::new(
        "http://127.0.0.1:1",
        "test-key",
        "minum-security-llm",
    ))
    .expect("模型创建成功");

    let completion = model
        .parse_response_body(
            r#"{
                    "usage": {
                        "prompt_tokens": 80,
                        "completion_tokens": 20,
                        "total_tokens": 100,
                        "prompt_tokens_details": {
                            "cached_tokens": 64
                        }
                    },
                    "choices": [
                        {
                            "message": {
                                "role": "assistant",
                                "content": "命中缓存",
                                "tool_calls": []
                            },
                            "finish_reason": "stop"
                        }
                    ]
                }"#,
        )
        .expect("响应解析成功");

    assert_eq!(completion.usage.as_ref().map(|usage| usage.cached_tokens), Some(64));
}

#[test]
fn 聊天补全可通过本地假服务完成一次真实调用() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("读取请求成功");

        let body = [
            r#"data: {"choices":[{"delta":{"content":"来自聊天补全假服务"}}]}"#,
            r#"data: [DONE]"#,
        ]
        .join("\n\n");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}\n\n",
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
    let completion = complete_model(&model, request).expect("调用成功");

    handle.join().expect("服务线程退出");
    assert_eq!(completion.plain_text(), "来自聊天补全假服务");
    assert_eq!(completion.stop_reason, CompletionStopReason::Stop);
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
        run_async(
            model.complete_streaming(request, &AbortSignal::new(), &mut |event| deltas.push(event)),
        )
        .expect("流式调用成功");

    handle.join().expect("服务线程退出");
    assert_eq!(completion.thinking_text(), Some("先分析".into()));
    assert_eq!(completion.plain_text(), "答案");
    assert_eq!(completion.stop_reason, CompletionStopReason::ToolUse);
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
            StreamEvent::ToolCallDetected {
                invocation_id: "call_1".into(),
                tool_name: "search_code".into(),
                arguments: Value::default(),
            },
            StreamEvent::Done,
        ]
    );
}
