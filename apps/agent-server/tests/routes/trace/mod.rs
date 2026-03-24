use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    TraceDashboardQuery, TraceListQuery,
    handlers::{get_trace, get_trace_dashboard, get_trace_overview, list_traces},
};
use crate::routes::test_support::{
    seed_tool_trace_with_changes, seed_trace, seed_trace_with_request_kind,
    seed_trace_with_request_kind_and_status, test_state,
};

#[tokio::test(flavor = "current_thread")]
async fn list_traces_reads_trace_page_from_store() {
    let state = test_state();
    seed_trace(state.as_ref(), "loop-1-step-0", 1_000);
    seed_trace(state.as_ref(), "loop-1-step-1", 1_010);
    seed_trace(state.as_ref(), "loop-2-step-0", 2_000);

    let (status, Json(body)) = list_traces(
        State(state),
        Query(TraceListQuery { page: Some(1), page_size: Some(10), request_kind: None }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["items"][0]["id"], "trace-loop-loop-2");
    assert_eq!(body["items"][0]["llm_span_count"], 1);
    assert_eq!(body["items"][1]["id"], "trace-loop-loop-1");
    assert_eq!(body["items"][1]["llm_span_count"], 2);
    assert_eq!(body["total_items"], 2);
    assert_eq!(body["page"], 1);
    assert_eq!(body["page_size"], 10);
}

#[tokio::test(flavor = "current_thread")]
async fn list_traces_can_filter_compression_logs() {
    let state = test_state();
    seed_trace(state.as_ref(), "trace-chat", 1_000);
    seed_trace_with_request_kind(state.as_ref(), "trace-compression", 2_000, "compression");

    let (status, Json(body)) = list_traces(
        State(state),
        Query(TraceListQuery {
            page: Some(1),
            page_size: Some(10),
            request_kind: Some("compression".into()),
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["items"][0]["id"], "trace-loop-trace-compression");
    assert_eq!(body["items"][0]["request_kind"], "compression");
    assert_eq!(body["total_items"], 1);
}

#[tokio::test(flavor = "current_thread")]
async fn get_trace_overview_returns_summary_and_page_together() {
    let state = test_state();
    seed_trace(state.as_ref(), "trace-chat-step-0", 1_000);
    seed_trace(state.as_ref(), "trace-chat-step-1", 1_020);

    let (status, Json(body)) = get_trace_overview(
        State(state),
        Query(TraceListQuery {
            page: Some(1),
            page_size: Some(10),
            request_kind: Some("completion".into()),
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["summary"]["total_requests"], 1);
    assert_eq!(body["summary"]["total_llm_spans"], 2);
    assert_eq!(body["summary"]["unique_models"], 1);
    assert_eq!(body["page"]["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["page"]["total_items"], 1);
    assert_eq!(body["page"]["items"][0]["llm_span_count"], 2);
    assert_eq!(body["page"]["items"][0]["traces"].as_array().map(Vec::len), Some(2));
}

#[tokio::test(flavor = "current_thread")]
async fn get_trace_returns_not_found_for_missing_id() {
    let state = test_state();

    let (status, Json(body)) = get_trace(State(state), Path("missing-trace".into())).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "trace 不存在：missing-trace");
}

#[tokio::test(flavor = "current_thread")]
async fn get_trace_can_return_loop_detail_by_loop_id() {
    let state = test_state();
    seed_trace(state.as_ref(), "trace-chat-step-0", 1_000);
    seed_trace(state.as_ref(), "trace-chat-step-1", 1_020);

    let (status, Json(body)) = get_trace(State(state), Path("trace-loop-trace-chat".into())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["loop_item"]["trace_id"], "trace-loop-trace-chat");
    assert_eq!(body["trace_details"].as_array().map(Vec::len), Some(2));
}

#[tokio::test(flavor = "current_thread")]
async fn get_trace_dashboard_returns_metrics_trend_and_activity() {
    let state = test_state();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis() as u64;
    seed_trace(
        state.as_ref(),
        "trace-dashboard-a-step-0",
        now_ms.saturating_sub(3 * 60 * 60 * 1000),
    );
    seed_trace(state.as_ref(), "trace-dashboard-b-step-0", now_ms.saturating_sub(60 * 60 * 1000));
    seed_trace_with_request_kind_and_status(
        state.as_ref(),
        "trace-dashboard-c-step-0",
        now_ms.saturating_sub(30 * 60 * 1000),
        "completion",
        agent_store::LlmTraceStatus::Failed,
    );
    seed_tool_trace_with_changes(
        state.as_ref(),
        "trace-dashboard-a",
        now_ms.saturating_sub(3 * 60 * 60 * 1000).saturating_add(50),
        "ApplyPatch",
        7,
        3,
    );

    let (status, Json(body)) = get_trace_dashboard(
        State(state),
        Query(TraceDashboardQuery { range: Some("today".into()) }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["range"], "today");
    assert_eq!(body["current"]["total_requests"], 3);
    assert_eq!(body["current"]["failed_requests"], 1);
    assert_eq!(body["current"]["partial_requests"], 0);
    assert_eq!(body["current"]["total_sessions"], 3);
    assert_eq!(body["current"]["total_input_tokens"], 30);
    assert_eq!(body["current"]["total_output_tokens"], 15);
    assert_eq!(body["current"]["total_cached_tokens"], 6);
    assert_eq!(body["current"]["total_tokens"], 45);
    assert_eq!(body["current"]["total_lines_changed"], 10);
    assert!(body["current"]["total_cost_usd"].as_f64().unwrap_or(0.0) > 0.0);
    assert_eq!(body["overall_summary"]["total_requests"], 3);
    assert_eq!(body["overall_summary"]["failed_requests"], 1);
    assert!(body["trend"].as_array().is_some_and(|items| !items.is_empty()));
    assert!(body["activity"].as_array().is_some_and(|items| !items.is_empty()));
}
