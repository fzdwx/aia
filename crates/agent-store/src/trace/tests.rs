use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use super::{
    LlmTraceDashboardRange, LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus,
    LlmTraceStore,
};
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
        session_id: Some("session-1".into()),
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

    let summary = store.summary().expect("summary should succeed");
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.failed_requests, 0);
    assert_eq!(summary.partial_requests, 0);
    assert_eq!(summary.total_llm_spans, 1);
    assert_eq!(summary.total_tool_spans, 0);
    assert_eq!(summary.requests_with_tools, 0);
    assert_eq!(summary.failed_tool_calls, 0);
    assert_eq!(summary.unique_models, 1);
    assert_eq!(summary.latest_request_started_at_ms, Some(100));
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
        session_id: Some("session-poisoned".into()),
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
    assert_eq!(summary.total_llm_spans, 1);
    assert_eq!(summary.unique_models, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn overview_returns_loop_page_instead_of_single_spans() {
    let store = Arc::new(AiaStore::in_memory().expect("store should initialize"));

    for step_index in 0..2_u32 {
        store
            .record_async(LlmTraceRecord {
                id: format!("trace-step-{step_index}"),
                trace_id: "loop-1".into(),
                span_id: format!("span-step-{step_index}"),
                parent_span_id: Some("root-1".into()),
                root_span_id: "root-1".into(),
                operation_name: "chat".into(),
                span_kind: LlmTraceSpanKind::Client,
                session_id: Some("session-loop-1".into()),
                turn_id: "turn-1".into(),
                run_id: "turn-1".into(),
                request_kind: "completion".into(),
                step_index,
                provider: "openai".into(),
                protocol: "openai-responses".into(),
                model: "gpt-5.4".into(),
                base_url: "https://api.example.com".into(),
                endpoint_path: "/responses".into(),
                streaming: false,
                started_at_ms: 100 + u64::from(step_index) * 10,
                finished_at_ms: Some(105 + u64::from(step_index) * 10),
                duration_ms: Some(5),
                status_code: Some(200),
                status: LlmTraceStatus::Succeeded,
                stop_reason: Some("stop".into()),
                error: None,
                request_summary: json!({"user_message": "hello loop"}),
                provider_request: json!({}),
                response_summary: json!({}),
                response_body: None,
                input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
                cached_tokens: Some(1),
                otel_attributes: json!({}),
                events: vec![],
            })
            .await
            .expect("record async");
    }

    let overview =
        store.overview_by_request_kind_async(10, 0, "completion").await.expect("overview async");

    assert_eq!(overview.summary.total_requests, 1);
    assert_eq!(overview.summary.total_llm_spans, 2);
    assert_eq!(overview.summary.total_tool_spans, 0);
    assert_eq!(overview.summary.unique_models, 1);
    assert_eq!(overview.summary.total_tokens, 30);
    assert_eq!(overview.page.total_items, 1);
    assert_eq!(overview.page.items.len(), 1);
    assert_eq!(overview.page.items[0].trace_id, "loop-1");
    assert_eq!(overview.page.items[0].llm_span_count, 2);
    assert_eq!(overview.page.items[0].traces.len(), 2);
    assert_eq!(overview.page.items[0].total_tokens, 30);
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
        session_id: Some("session-async".into()),
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

    let page = store.list_loop_page_async(10, 0).await.expect("list loop page async");
    assert_eq!(page.items.len(), 1);

    let loaded = store.get_async("trace-async").await.expect("get async").expect("trace exists");
    assert_eq!(loaded, record);

    let overview =
        store.overview_by_request_kind_async(10, 0, "completion").await.expect("overview async");
    assert_eq!(overview.summary.total_requests, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn async_trace_filters_can_separate_compression_logs() {
    let store = Arc::new(AiaStore::in_memory().expect("store should initialize"));

    for (id, request_kind, started_at_ms) in
        [("trace-chat", "completion", 100_u64), ("trace-compression", "compression", 200_u64)]
    {
        store
            .record_async(LlmTraceRecord {
                id: id.into(),
                trace_id: format!("{id}-group"),
                span_id: id.into(),
                parent_span_id: Some(format!("{id}-root")),
                root_span_id: format!("{id}-root"),
                operation_name: if request_kind == "compression" {
                    "summarize".into()
                } else {
                    "chat".into()
                },
                span_kind: LlmTraceSpanKind::Client,
                session_id: Some(format!("session-{id}")),
                turn_id: format!("turn-{id}"),
                run_id: format!("run-{id}"),
                request_kind: request_kind.into(),
                step_index: 0,
                provider: "openai".into(),
                protocol: "openai-responses".into(),
                model: "gpt-5.4".into(),
                base_url: "https://api.example.com".into(),
                endpoint_path: "/responses".into(),
                streaming: false,
                started_at_ms,
                finished_at_ms: Some(started_at_ms + 25),
                duration_ms: Some(25),
                status_code: Some(200),
                status: LlmTraceStatus::Succeeded,
                stop_reason: Some("stop".into()),
                error: None,
                request_summary: json!({
                    "user_message": if request_kind == "compression" {
                        serde_json::Value::Null
                    } else {
                        json!("hello")
                    }
                }),
                provider_request: json!({"model": "gpt-5.4"}),
                response_summary: json!({}),
                response_body: Some(if request_kind == "compression" {
                    "压缩摘要：旧历史已压缩。".into()
                } else {
                    "普通回复".into()
                }),
                input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
                cached_tokens: Some(0),
                otel_attributes: json!({}),
                events: vec![],
            })
            .await
            .expect("record async");
    }

    let compression_page = store
        .list_loop_page_by_request_kind_async(10, 0, "compression")
        .await
        .expect("compression page async");
    assert_eq!(compression_page.total_items, 1);
    assert_eq!(compression_page.items.len(), 1);
    assert_eq!(compression_page.items[0].request_kind, "compression");

    let conversation_page = store
        .list_loop_page_by_request_kind_async(10, 0, "completion")
        .await
        .expect("conversation page async");
    assert_eq!(conversation_page.total_items, 1);
    assert_eq!(conversation_page.items.len(), 1);
    assert_eq!(conversation_page.items[0].request_kind, "completion");

    let compression_overview = store
        .overview_by_request_kind_async(10, 0, "compression")
        .await
        .expect("compression overview async");
    assert_eq!(compression_overview.summary.total_requests, 1);
    assert_eq!(compression_overview.summary.total_tokens, 15);
}

#[test]
fn summary_rollup_updates_same_loop_without_double_counting_requests() {
    let store = AiaStore::in_memory().expect("store should initialize");
    let build_record = |id: &str,
                        span_id: &str,
                        step_index: u32,
                        started_at_ms: u64,
                        duration_ms: u64,
                        input_tokens: u64,
                        output_tokens: u64,
                        total_tokens: u64,
                        cached_tokens: u64| LlmTraceRecord {
        id: id.into(),
        trace_id: "loop-incremental".into(),
        span_id: span_id.into(),
        parent_span_id: Some("loop-incremental-root".into()),
        root_span_id: "loop-incremental-root".into(),
        operation_name: "chat".into(),
        span_kind: LlmTraceSpanKind::Client,
        session_id: Some("session-incremental".into()),
        turn_id: "turn-incremental".into(),
        run_id: "run-incremental".into(),
        request_kind: "completion".into(),
        step_index,
        provider: "openai".into(),
        protocol: "openai-responses".into(),
        model: "gpt-5.4".into(),
        base_url: "https://api.example.com".into(),
        endpoint_path: "/responses".into(),
        streaming: false,
        started_at_ms,
        finished_at_ms: Some(started_at_ms + duration_ms),
        duration_ms: Some(duration_ms),
        status_code: Some(200),
        status: LlmTraceStatus::Succeeded,
        stop_reason: Some("stop".into()),
        error: None,
        request_summary: json!({"user_message": "hello"}),
        provider_request: json!({}),
        response_summary: json!({}),
        response_body: None,
        input_tokens: Some(input_tokens),
        output_tokens: Some(output_tokens),
        total_tokens: Some(total_tokens),
        cached_tokens: Some(cached_tokens),
        otel_attributes: json!({}),
        events: vec![],
    };

    store
        .record(&build_record("loop-step-0", "loop-step-0", 0, 100, 80, 12, 6, 18, 4))
        .expect("first record should persist");
    store
        .record(&build_record("loop-step-1", "loop-step-1", 1, 200, 20, 5, 3, 8, 0))
        .expect("second record should persist");

    let summary = store.summary().expect("summary should succeed");
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.total_llm_spans, 2);
    assert_eq!(summary.total_input_tokens, 17);
    assert_eq!(summary.total_output_tokens, 9);
    assert_eq!(summary.total_tokens, 26);
    assert_eq!(summary.total_cached_tokens, 4);
    assert_eq!(summary.avg_duration_ms, Some(120.0));
    assert_eq!(summary.p95_duration_ms, Some(120));
}

#[test]
fn summary_rollup_updates_tool_counts_without_double_counting_requests_with_tools() {
    let store = AiaStore::in_memory().expect("store should initialize");

    store
        .record(&LlmTraceRecord {
            id: "loop-tools-client".into(),
            trace_id: "loop-tools".into(),
            span_id: "loop-tools-client".into(),
            parent_span_id: Some("loop-tools-root".into()),
            root_span_id: "loop-tools-root".into(),
            operation_name: "chat".into(),
            span_kind: LlmTraceSpanKind::Client,
            session_id: Some("session-tools".into()),
            turn_id: "turn-tools".into(),
            run_id: "run-tools".into(),
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
            request_summary: json!({"user_message": "needs tools"}),
            provider_request: json!({}),
            response_summary: json!({}),
            response_body: None,
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cached_tokens: Some(0),
            otel_attributes: json!({}),
            events: vec![],
        })
        .expect("client record should persist");

    for step_index in 1..=2_u32 {
        store
            .record(&LlmTraceRecord {
                id: format!("loop-tools-tool-{step_index}"),
                trace_id: "loop-tools".into(),
                span_id: format!("loop-tools-tool-{step_index}"),
                parent_span_id: Some("loop-tools-client".into()),
                root_span_id: "loop-tools-root".into(),
                operation_name: "tool".into(),
                span_kind: LlmTraceSpanKind::Internal,
                session_id: Some("session-tools".into()),
                turn_id: "turn-tools".into(),
                run_id: "run-tools".into(),
                request_kind: "tool".into(),
                step_index,
                provider: "builtin".into(),
                protocol: "local".into(),
                model: "tool-executor".into(),
                base_url: "local".into(),
                endpoint_path: "/tool".into(),
                streaming: false,
                started_at_ms: 180 + u64::from(step_index) * 10,
                finished_at_ms: Some(185 + u64::from(step_index) * 10),
                duration_ms: Some(5),
                status_code: None,
                status: LlmTraceStatus::Failed,
                stop_reason: None,
                error: Some("tool failed".into()),
                request_summary: json!({}),
                provider_request: json!({}),
                response_summary: json!({}),
                response_body: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cached_tokens: None,
                otel_attributes: json!({}),
                events: vec![],
            })
            .expect("tool record should persist");
    }

    let summary = store.summary().expect("summary should succeed");
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.partial_requests, 1);
    assert_eq!(summary.failed_requests, 0);
    assert_eq!(summary.total_tool_spans, 2);
    assert_eq!(summary.requests_with_tools, 1);
    assert_eq!(summary.failed_tool_calls, 2);
}

#[test]
fn mixed_non_tool_loop_shapes_are_rejected() {
    let store = AiaStore::in_memory().expect("store should initialize");

    store
        .record(&LlmTraceRecord {
            id: "mixed-loop-0".into(),
            trace_id: "mixed-loop".into(),
            span_id: "mixed-loop-0".into(),
            parent_span_id: Some("mixed-root".into()),
            root_span_id: "mixed-root".into(),
            operation_name: "chat".into(),
            span_kind: LlmTraceSpanKind::Client,
            session_id: Some("session-mixed".into()),
            turn_id: "turn-mixed".into(),
            run_id: "run-mixed".into(),
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
            provider_request: json!({}),
            response_summary: json!({}),
            response_body: None,
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cached_tokens: Some(0),
            otel_attributes: json!({}),
            events: vec![],
        })
        .expect("first record should persist");

    let error = store
        .record(&LlmTraceRecord {
            id: "mixed-loop-1".into(),
            trace_id: "mixed-loop".into(),
            span_id: "mixed-loop-1".into(),
            parent_span_id: Some("mixed-root".into()),
            root_span_id: "mixed-root".into(),
            operation_name: "compress".into(),
            span_kind: LlmTraceSpanKind::Client,
            session_id: Some("session-mixed".into()),
            turn_id: "turn-mixed".into(),
            run_id: "run-mixed".into(),
            request_kind: "compression".into(),
            step_index: 1,
            provider: "openai".into(),
            protocol: "openai-responses".into(),
            model: "gpt-5.4-mini".into(),
            base_url: "https://api.example.com".into(),
            endpoint_path: "/responses".into(),
            streaming: false,
            started_at_ms: 200,
            finished_at_ms: Some(220),
            duration_ms: Some(20),
            status_code: Some(200),
            status: LlmTraceStatus::Succeeded,
            stop_reason: Some("stop".into()),
            error: None,
            request_summary: json!({}),
            provider_request: json!({}),
            response_summary: json!({}),
            response_body: None,
            input_tokens: Some(2),
            output_tokens: Some(1),
            total_tokens: Some(3),
            cached_tokens: Some(0),
            otel_attributes: json!({}),
            events: vec![],
        })
        .expect_err("mixed loop should be rejected");

    assert!(error.to_string().contains("mixed non-tool request kinds"));

    let summary = store.summary().expect("summary should stay consistent");
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.unique_models, 1);
    assert_eq!(summary.total_tokens, 15);
}

#[test]
fn dashboard_activity_rollup_tracks_day_moves_distinct_sessions_and_backfill() {
    let store = AiaStore::in_memory().expect("store should initialize");
    let day_ms = 24 * 60 * 60 * 1000_u64;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis() as u64;
    let build_record = |id: &str,
                        trace_id: &str,
                        session_id: &str,
                        step_index: u32,
                        started_at_ms: u64,
                        total_tokens: u64| LlmTraceRecord {
        id: id.into(),
        trace_id: trace_id.into(),
        span_id: id.into(),
        parent_span_id: Some(format!("{trace_id}-root")),
        root_span_id: format!("{trace_id}-root"),
        operation_name: "chat".into(),
        span_kind: LlmTraceSpanKind::Client,
        session_id: Some(session_id.into()),
        turn_id: format!("turn-{trace_id}"),
        run_id: format!("run-{trace_id}"),
        request_kind: "completion".into(),
        step_index,
        provider: "openai".into(),
        protocol: "openai-responses".into(),
        model: "gpt-5.4".into(),
        base_url: "https://api.example.com".into(),
        endpoint_path: "/responses".into(),
        streaming: false,
        started_at_ms,
        finished_at_ms: Some(started_at_ms + 25),
        duration_ms: Some(25),
        status_code: Some(200),
        status: LlmTraceStatus::Succeeded,
        stop_reason: Some("stop".into()),
        error: None,
        request_summary: json!({"user_message": "dashboard activity"}),
        provider_request: json!({}),
        response_summary: json!({}),
        response_body: None,
        input_tokens: Some(total_tokens.saturating_sub(5)),
        output_tokens: Some(5),
        total_tokens: Some(total_tokens),
        cached_tokens: Some(0),
        otel_attributes: json!({}),
        events: vec![],
    };

    let day_one = now_ms.saturating_sub(2 * day_ms);
    let day_two = now_ms.saturating_sub(day_ms);

    store
        .record(&build_record("loop-a-step-0", "loop-a", "session-1", 0, day_one + 100, 15))
        .expect("first loop record should persist");
    store
        .record(&build_record("loop-a-step-1", "loop-a", "session-1", 1, day_two + 200, 24))
        .expect("loop update should persist");
    store
        .record(&build_record("loop-b-step-0", "loop-b", "session-1", 0, day_two + 400, 18))
        .expect("same-session loop should persist");
    store
        .record(&build_record("loop-c-step-0", "loop-c", "session-2", 0, day_two + 800, 30))
        .expect("second-session loop should persist");

    let dashboard = store
        .trace_dashboard(LlmTraceDashboardRange::Month)
        .expect("dashboard query should succeed");
    let day_one_bucket = (day_one / day_ms) * day_ms;
    let day_two_bucket = (day_two / day_ms) * day_ms;
    let day_one_point = dashboard
        .activity
        .iter()
        .find(|point| point.day_start_ms == day_one_bucket)
        .expect("day one bucket should exist");
    let day_two_point = dashboard
        .activity
        .iter()
        .find(|point| point.day_start_ms == day_two_bucket)
        .expect("day two bucket should exist");

    assert_eq!(day_one_point.total_requests, 0);
    assert_eq!(day_one_point.total_sessions, 0);
    assert_eq!(day_two_point.total_requests, 3);
    assert_eq!(day_two_point.total_sessions, 2);
    assert_eq!(day_two_point.total_tokens, 87);

    store
        .with_conn(|conn| {
            conn.execute("DELETE FROM llm_trace_activity_daily", [])?;
            conn.execute("DELETE FROM llm_trace_activity_daily_sessions", [])?;
            Ok(())
        })
        .expect("clearing activity rollups should succeed");

    let rebuilt = store
        .trace_dashboard(LlmTraceDashboardRange::Month)
        .expect("dashboard should rebuild activity rollups");
    let rebuilt_day_two = rebuilt
        .activity
        .iter()
        .find(|point| point.day_start_ms == day_two_bucket)
        .expect("rebuilt day two bucket should exist");
    assert_eq!(rebuilt_day_two.total_requests, 3);
    assert_eq!(rebuilt_day_two.total_sessions, 2);
    assert_eq!(rebuilt_day_two.total_tokens, 87);
}
