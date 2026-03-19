use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use super::{
    dto::TraceListQuery,
    handlers::{get_trace, get_trace_overview, get_trace_summary, list_traces},
};
use crate::routes::test_support::{seed_trace, seed_trace_with_request_kind, test_state};

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
async fn get_trace_summary_returns_aggregate_counts() {
    let state = test_state();
    seed_trace(state.as_ref(), "trace-1", 1_000);
    seed_trace(state.as_ref(), "trace-2", 2_000);

    let (status, Json(body)) = get_trace_summary(
        State(state),
        Query(TraceListQuery { page: None, page_size: None, request_kind: None }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_requests"], 2);
    assert_eq!(body["failed_requests"], 0);
    assert_eq!(body["total_input_tokens"], 20);
    assert_eq!(body["total_output_tokens"], 10);
    assert_eq!(body["total_tokens"], 30);
    assert_eq!(body["total_cached_tokens"], 4);
}

#[tokio::test(flavor = "current_thread")]
async fn get_trace_summary_can_filter_compression_logs() {
    let state = test_state();
    seed_trace(state.as_ref(), "trace-chat", 1_000);
    seed_trace_with_request_kind(state.as_ref(), "trace-compression", 2_000, "compression");

    let (status, Json(body)) = get_trace_summary(
        State(state),
        Query(TraceListQuery {
            page: None,
            page_size: None,
            request_kind: Some("compression".into()),
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_requests"], 1);
    assert_eq!(body["total_tokens"], 15);
}
