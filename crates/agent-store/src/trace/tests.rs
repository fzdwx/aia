use std::sync::Arc;

use serde_json::json;

use super::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore};
use crate::AiaStore;

#[test]
fn store_records_round_trip_and_summary() {
    let store = AiaStore::in_memory().expect("store should initialize");
    let record = LlmTraceRecord {
        id: "trace-1".into(),
        trace_id: "trace-group-1".into(),
        span_id: "trace-1".into(),
        parent_span_id: Some("root-span-1".into()),
        root_span_id: "root-span-1".into(),
        operation_name: "chat".into(),
        span_kind: LlmTraceSpanKind::Client,
        turn_id: "turn-1".into(),
        run_id: "turn-1".into(),
        request_kind: "completion".into(),
        step_index: 0,
        provider: "openai".into(),
        protocol: "openai-responses".into(),
        model: "gpt-5.4".into(),
        base_url: "https://api.example.com".into(),
        endpoint_path: "/responses".into(),
        streaming: true,
        started_at_ms: 100,
        finished_at_ms: Some(180),
        duration_ms: Some(80),
        status_code: Some(200),
        status: LlmTraceStatus::Succeeded,
        stop_reason: Some("stop".into()),
        error: None,
        request_summary: json!({"conversation_items": 2}),
        provider_request: json!({"model": "gpt-5.4"}),
        response_summary: json!({"assistant_text": "你好"}),
        response_body: Some("你好".into()),
        input_tokens: Some(12),
        output_tokens: Some(6),
        total_tokens: Some(18),
        cached_tokens: Some(4),
        otel_attributes: json!({"gen_ai.operation.name": "chat"}),
        events: vec![LlmTraceEvent {
            name: "response.completed".into(),
            at_ms: 180,
            attributes: json!({"http.response.status_code": 200}),
        }],
    };

    store.record(&record).expect("record should persist");

    let loaded = store.get("trace-1").expect("query should succeed").expect("trace exists");
    assert_eq!(loaded, record);

    let list = store.list(10).expect("list should succeed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "trace-1");
    assert_eq!(list[0].status, LlmTraceStatus::Succeeded);
    assert_eq!(list[0].stop_reason.as_deref(), Some("stop"));
    assert_eq!(list[0].total_tokens, Some(18));
    assert_eq!(list[0].cached_tokens, Some(4));
    assert_eq!(list[0].user_message, None);

    let summary = store.summary().expect("summary should succeed");
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.failed_requests, 0);
    assert_eq!(summary.total_tokens, 18);
    assert_eq!(summary.total_cached_tokens, 4);
    assert_eq!(summary.p95_duration_ms, Some(80));
}

#[test]
fn trace_operations_recover_after_poisoned_mutex() {
    let store = Arc::new(AiaStore::in_memory().expect("store should initialize"));
    let cloned = store.clone();
    let _ = std::thread::spawn(move || {
        let _guard = cloned.conn.lock().expect("test should lock before poisoning");
        panic!("poison store mutex");
    })
    .join();

    let record = LlmTraceRecord {
        id: "trace-poisoned".into(),
        trace_id: "trace-poisoned-group".into(),
        span_id: "trace-poisoned".into(),
        parent_span_id: Some("trace-poisoned-root".into()),
        root_span_id: "trace-poisoned-root".into(),
        operation_name: "chat".into(),
        span_kind: LlmTraceSpanKind::Client,
        turn_id: "turn-poisoned".into(),
        run_id: "turn-poisoned".into(),
        request_kind: "completion".into(),
        step_index: 0,
        provider: "openai".into(),
        protocol: "openai-responses".into(),
        model: "gpt-5.4".into(),
        base_url: "https://api.example.com".into(),
        endpoint_path: "/responses".into(),
        streaming: false,
        started_at_ms: 100,
        finished_at_ms: Some(180),
        duration_ms: Some(80),
        status_code: Some(200),
        status: LlmTraceStatus::Succeeded,
        stop_reason: Some("stop".into()),
        error: None,
        request_summary: json!({}),
        provider_request: json!({"messages": [{"role": "user", "content": "recover"}]}),
        response_summary: json!({}),
        response_body: None,
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cached_tokens: None,
        otel_attributes: json!({}),
        events: vec![],
    };

    store.record(&record).expect("record should persist after poison");
    let loaded = store
        .get("trace-poisoned")
        .expect("query should succeed after poison")
        .expect("trace should exist after poison");
    assert_eq!(loaded.id, "trace-poisoned");
    let summary = store.summary().expect("summary should succeed after poison");
    assert_eq!(summary.total_requests, 1);
}

#[test]
fn list_extracts_user_message_from_chat_completions_request() {
    let store = AiaStore::in_memory().expect("store should initialize");
    store
        .record(&LlmTraceRecord {
            id: "trace-chat".into(),
            trace_id: "trace-chat-group".into(),
            span_id: "trace-chat".into(),
            parent_span_id: Some("trace-chat-root".into()),
            root_span_id: "trace-chat-root".into(),
            operation_name: "chat".into(),
            span_kind: LlmTraceSpanKind::Client,
            turn_id: "turn-chat".into(),
            run_id: "turn-chat".into(),
            request_kind: "completion".into(),
            step_index: 0,
            provider: "openai".into(),
            protocol: "openai-chat-completions".into(),
            model: "gpt-5.4".into(),
            base_url: "https://api.example.com".into(),
            endpoint_path: "/chat/completions".into(),
            streaming: false,
            started_at_ms: 100,
            finished_at_ms: Some(180),
            duration_ms: Some(80),
            status_code: Some(200),
            status: LlmTraceStatus::Succeeded,
            stop_reason: Some("stop".into()),
            error: None,
            request_summary: json!({}),
            provider_request: json!({
                "messages": [
                    {"role": "system", "content": "keep it short"},
                    {"role": "user", "content": "summarize this repo"}
                ]
            }),
            response_summary: json!({}),
            response_body: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cached_tokens: None,
            otel_attributes: json!({"gen_ai.operation.name": "chat"}),
            events: vec![],
        })
        .expect("record should persist");

    let list = store.list(10).expect("list should succeed");
    assert_eq!(list[0].user_message.as_deref(), Some("summarize this repo"));
}

#[test]
fn list_extracts_user_message_from_responses_request() {
    let store = AiaStore::in_memory().expect("store should initialize");
    store
        .record(&LlmTraceRecord {
            id: "trace-responses".into(),
            trace_id: "trace-responses-group".into(),
            span_id: "trace-responses".into(),
            parent_span_id: Some("trace-responses-root".into()),
            root_span_id: "trace-responses-root".into(),
            operation_name: "chat".into(),
            span_kind: LlmTraceSpanKind::Client,
            turn_id: "turn-responses".into(),
            run_id: "turn-responses".into(),
            request_kind: "completion".into(),
            step_index: 0,
            provider: "openai".into(),
            protocol: "openai-responses".into(),
            model: "gpt-5.4".into(),
            base_url: "https://api.example.com".into(),
            endpoint_path: "/responses".into(),
            streaming: false,
            started_at_ms: 100,
            finished_at_ms: Some(180),
            duration_ms: Some(80),
            status_code: Some(200),
            status: LlmTraceStatus::Succeeded,
            stop_reason: Some("stop".into()),
            error: None,
            request_summary: json!({}),
            provider_request: json!({
                "input": [
                    {"role": "system", "content": "keep it short"},
                    {"role": "user", "content": "explain the failing test"}
                ]
            }),
            response_summary: json!({}),
            response_body: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cached_tokens: None,
            otel_attributes: json!({"gen_ai.operation.name": "chat"}),
            events: vec![],
        })
        .expect("record should persist");

    let list = store.list(10).expect("list should succeed");
    assert_eq!(list[0].user_message.as_deref(), Some("explain the failing test"));
}

#[test]
fn list_page_paginates_by_loop_not_individual_span() {
    let store = AiaStore::in_memory().expect("store should initialize");

    for (loop_index, started_at_ms) in [(1_u32, 300_u64), (2_u32, 200_u64), (3_u32, 100_u64)] {
        for step_index in 0..2_u32 {
            store
                .record(&LlmTraceRecord {
                    id: format!("trace-{loop_index}-{step_index}"),
                    trace_id: format!("loop-{loop_index}"),
                    span_id: format!("span-{loop_index}-{step_index}"),
                    parent_span_id: Some(format!("root-{loop_index}")),
                    root_span_id: format!("root-{loop_index}"),
                    operation_name: "chat".into(),
                    span_kind: LlmTraceSpanKind::Client,
                    turn_id: format!("turn-{loop_index}"),
                    run_id: format!("run-{loop_index}"),
                    request_kind: "completion".into(),
                    step_index,
                    provider: "openai".into(),
                    protocol: "openai-responses".into(),
                    model: "gpt-5".into(),
                    base_url: "https://api.example.com".into(),
                    endpoint_path: "/responses".into(),
                    streaming: false,
                    started_at_ms: started_at_ms + u64::from(step_index),
                    finished_at_ms: Some(started_at_ms + u64::from(step_index) + 10),
                    duration_ms: Some(10),
                    status_code: Some(200),
                    status: LlmTraceStatus::Succeeded,
                    stop_reason: Some("stop".into()),
                    error: None,
                    request_summary: json!({}),
                    provider_request: json!({
                        "input": [{"role": "user", "content": format!("message {loop_index}")}]
                    }),
                    response_summary: json!({}),
                    response_body: None,
                    input_tokens: None,
                    output_tokens: None,
                    total_tokens: None,
                    cached_tokens: None,
                    otel_attributes: json!({}),
                    events: vec![],
                })
                .expect("record should persist");
        }
    }

    let first_page = store.list_page(2, 0).expect("page query should succeed");
    assert_eq!(first_page.total_loops, 3);
    assert_eq!(first_page.page, 1);
    assert_eq!(first_page.page_size, 2);
    assert_eq!(first_page.items.len(), 4);
    assert!(
        first_page.items.iter().all(|item| item.trace_id == "loop-1" || item.trace_id == "loop-2")
    );
    assert!(first_page.items.iter().any(|item| item.trace_id == "loop-1"));
    assert!(first_page.items.iter().any(|item| item.trace_id == "loop-2"));
    assert!(first_page.items.iter().all(|item| item.trace_id != "loop-3"));

    let second_page = store.list_page(2, 2).expect("page query should succeed");
    assert_eq!(second_page.total_loops, 3);
    assert_eq!(second_page.page, 2);
    assert_eq!(second_page.page_size, 2);
    assert_eq!(second_page.items.len(), 2);
    assert!(second_page.items.iter().all(|item| item.trace_id == "loop-3"));
}

#[tokio::test(flavor = "current_thread")]
async fn async_trace_methods_work() {
    let store = Arc::new(AiaStore::in_memory().expect("store should initialize"));
    let record = LlmTraceRecord {
        id: "trace-async".into(),
        trace_id: "trace-async-group".into(),
        span_id: "trace-async".into(),
        parent_span_id: Some("trace-async-root".into()),
        root_span_id: "trace-async-root".into(),
        operation_name: "chat".into(),
        span_kind: LlmTraceSpanKind::Client,
        turn_id: "turn-async".into(),
        run_id: "turn-async".into(),
        request_kind: "completion".into(),
        step_index: 0,
        provider: "openai".into(),
        protocol: "openai-responses".into(),
        model: "gpt-5.4".into(),
        base_url: "https://api.example.com".into(),
        endpoint_path: "/responses".into(),
        streaming: true,
        started_at_ms: 100,
        finished_at_ms: Some(180),
        duration_ms: Some(80),
        status_code: Some(200),
        status: LlmTraceStatus::Succeeded,
        stop_reason: Some("stop".into()),
        error: None,
        request_summary: json!({"conversation_items": 1}),
        provider_request: json!({"model": "gpt-5.4"}),
        response_summary: json!({"assistant_text": "你好"}),
        response_body: Some("你好".into()),
        input_tokens: Some(12),
        output_tokens: Some(6),
        total_tokens: Some(18),
        cached_tokens: Some(4),
        otel_attributes: json!({}),
        events: vec![],
    };

    store.record_async(record.clone()).await.expect("record async");

    let page = store.list_page_async(10, 0).await.expect("list page async");
    assert_eq!(page.items.len(), 1);

    let loaded = store.get_async("trace-async").await.expect("get async").expect("trace exists");
    assert_eq!(loaded, record);

    let summary = store.summary_async().await.expect("summary async");
    assert_eq!(summary.total_requests, 1);
}
