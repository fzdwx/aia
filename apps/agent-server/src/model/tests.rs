use std::{
    future::Future,
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
};

use agent_core::{
    AbortSignal, CompletionRequest, ConversationItem, LanguageModel, Message, ModelDisposition,
    ModelIdentity, Role,
};
use agent_store::{AiaStore, LlmTraceStatus, LlmTraceStore};
use provider_registry::{ModelConfig, ModelLimit, ProviderKind, ProviderProfile};
use serde_json::json;

use super::{ProviderLaunchChoice, ServerModel, build_model_from_selection};

fn run_async<T>(future: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build");
    runtime.block_on(future)
}

#[test]
fn server_model_marks_cancelled_openai_errors_as_cancelled() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept should succeed");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("request should be readable");

        let response =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        stream.write_all(response.as_bytes()).expect("response write should succeed");
        stream.flush().expect("response flush should succeed");
        thread::sleep(std::time::Duration::from_millis(120));
        let _ = stream.write_all(b"data: [DONE]\n\n");
    });

    let profile = ProviderProfile {
        name: "rayin".to_string(),
        kind: ProviderKind::OpenAiResponses,
        base_url: format!("http://{address}"),
        api_key: "test-key".to_string(),
        models: vec![ModelConfig {
            id: "gpt-5.4".to_string(),
            display_name: None,
            limit: Some(ModelLimit { context: Some(200_000), output: Some(8_192) }),
            default_temperature: None,
            supports_reasoning: false,
            reasoning_effort: None,
        }],
        active_model: Some("gpt-5.4".to_string()),
    };

    let (_, model) = build_model_from_selection(ProviderLaunchChoice::OpenAi(profile), None)
        .expect("model should build");

    let abort = AbortSignal::new();
    let cancel = abort.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(30));
        cancel.abort();
    });

    let error = run_async(model.complete_streaming_with_abort(
        CompletionRequest {
            model: ModelIdentity::new("openai", "gpt-5.4", ModelDisposition::Balanced),
            instructions: Some("保持简洁".into()),
            conversation: vec![ConversationItem::Message(Message::new(Role::User, "hi"))],
            max_output_tokens: Some(128),
            available_tools: vec![],
            prompt_cache: None,
            user_agent: Some("aia-test/1.0".into()),
            timeout: None,
            trace_context: None,
        },
        &abort,
        &mut |_| {},
    ))
    .expect_err("completion should be cancelled");

    handle.join().expect("server thread should exit");
    assert!(ServerModel::is_cancelled_error(&error));
}

#[test]
fn responses_model_call_writes_llm_trace_record() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept should succeed");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("request should be readable");

        let body = [
            r#"data: {"type":"response.created","response":{"id":"resp_1"}}"#,
            r#"data: {"type":"response.output_text.delta","delta":"trace ok"}"#,
            r#"data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","usage":{"input_tokens":21,"output_tokens":9,"total_tokens":30}}}"#,
            r#"data: [DONE]"#,
        ]
        .join("\n\n");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}\n\n",
            body
        );
        stream.write_all(response.as_bytes()).expect("response write should succeed");
    });

    let store = Arc::new(AiaStore::in_memory().expect("trace store should init"));
    let profile = ProviderProfile {
        name: "rayin".to_string(),
        kind: ProviderKind::OpenAiResponses,
        base_url: format!("http://{address}"),
        api_key: "test-key".to_string(),
        models: vec![ModelConfig {
            id: "gpt-5.4".to_string(),
            display_name: None,
            limit: Some(ModelLimit { context: Some(200_000), output: Some(8_192) }),
            default_temperature: None,
            supports_reasoning: false,
            reasoning_effort: None,
        }],
        active_model: Some("gpt-5.4".to_string()),
    };

    let (identity, model) =
        build_model_from_selection(ProviderLaunchChoice::OpenAi(profile), Some(store.clone()))
            .expect("model should build");

    let completion = run_async(model.complete_streaming(
        CompletionRequest {
            model: ModelIdentity::new("openai", "gpt-5.4", ModelDisposition::Balanced),
            instructions: Some("保持简洁".into()),
            conversation: vec![ConversationItem::Message(Message::new(Role::User, "hi"))],
            max_output_tokens: Some(128),
            available_tools: vec![],
            prompt_cache: None,
            user_agent: Some("aia-test/1.0".into()),
            timeout: None,
            trace_context: Some(agent_core::LlmTraceRequestContext {
                trace_id: "aia-trace-turn-1".into(),
                span_id: "trace-1".into(),
                parent_span_id: Some("aia-span-turn-1-root".into()),
                root_span_id: "aia-span-turn-1-root".into(),
                operation_name: "chat".into(),
                turn_id: "turn-1".into(),
                run_id: "turn-1".into(),
                request_kind: "completion".into(),
                step_index: 0,
            }),
        },
        &mut |_| {},
    ))
    .expect("completion should succeed");

    handle.join().expect("server thread should exit");
    assert_eq!(identity.name, "gpt-5.4");
    assert_eq!(completion.plain_text(), "trace ok");

    let trace = store.get("trace-1").expect("trace query should succeed").expect("trace exists");
    assert_eq!(trace.trace_id, "aia-trace-turn-1");
    assert_eq!(trace.parent_span_id.as_deref(), Some("aia-span-turn-1-root"));
    assert_eq!(trace.operation_name, "chat");
    assert_eq!(trace.model, "gpt-5.4");
    assert_eq!(trace.endpoint_path, "/responses");
    assert_eq!(trace.status_code, Some(200));
    assert_eq!(trace.input_tokens, Some(21));
    assert_eq!(trace.output_tokens, Some(9));
    assert_eq!(trace.total_tokens, Some(30));
    assert_eq!(trace.cached_tokens, Some(0));
    assert_eq!(
        trace.otel_attributes.get("http.request.header.user_agent"),
        Some(&json!("aia-test/1.0"))
    );
    assert!(trace.response_body.as_deref().is_some_and(|body| body.contains("response.completed")));
}

#[test]
fn responses_http_502_writes_failed_trace_record() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept should succeed");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("request should be readable");

        let body = r#"{"error":"gateway failure"}"#;
        let response = format!(
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("response write should succeed");
    });

    let store = Arc::new(AiaStore::in_memory().expect("trace store should init"));
    let profile = ProviderProfile {
        name: "rayin".to_string(),
        kind: ProviderKind::OpenAiResponses,
        base_url: format!("http://{address}"),
        api_key: "test-key".to_string(),
        models: vec![ModelConfig {
            id: "gpt-5.4".to_string(),
            display_name: None,
            limit: Some(ModelLimit { context: Some(200_000), output: Some(8_192) }),
            default_temperature: None,
            supports_reasoning: false,
            reasoning_effort: None,
        }],
        active_model: Some("gpt-5.4".to_string()),
    };

    let (_identity, model) =
        build_model_from_selection(ProviderLaunchChoice::OpenAi(profile), Some(store.clone()))
            .expect("model should build");

    let error = run_async(model.complete(CompletionRequest {
        model: ModelIdentity::new("openai", "gpt-5.4", ModelDisposition::Balanced),
        instructions: Some("保持简洁".into()),
        conversation: vec![ConversationItem::Message(Message::new(Role::User, "hi"))],
        max_output_tokens: Some(128),
        available_tools: vec![],
        prompt_cache: None,
        user_agent: Some("aia-test/1.0".into()),
        timeout: None,
        trace_context: Some(agent_core::LlmTraceRequestContext {
            trace_id: "aia-trace-turn-1".into(),
            span_id: "trace-502".into(),
            parent_span_id: Some("aia-span-turn-1-root".into()),
            root_span_id: "aia-span-turn-1-root".into(),
            operation_name: "chat".into(),
            turn_id: "turn-1".into(),
            run_id: "turn-1".into(),
            request_kind: "completion".into(),
            step_index: 0,
        }),
    }))
    .expect_err("completion should fail");

    handle.join().expect("server thread should exit");
    assert!(error.to_string().contains("502"));

    let trace = store.get("trace-502").expect("trace query should succeed").expect("trace exists");
    assert_eq!(trace.status, LlmTraceStatus::Failed);
    assert_eq!(trace.status_code, Some(502));
    assert!(trace.response_body.as_deref().is_some_and(|body| body.contains("gateway failure")));
}
