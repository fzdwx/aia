use std::sync::{Arc, RwLock};

use agent_store::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use channel_registry::ChannelRegistry;
use provider_registry::{ModelConfig, ModelLimit, ProviderRegistry};

use super::{
    channel::{CreateChannelRequest, UpdateChannelRequest},
    common::{json_response, resolve_session_id},
    provider::{ModelConfigDto, ModelLimitDto},
    trace::{TraceListQuery, get_trace, get_trace_overview, get_trace_summary, list_traces},
    turn::CancelTurnRequest,
};
use crate::{
    session_manager::{ProviderInfoSnapshot, SessionManagerHandle},
    state::AppState,
};

fn test_state() -> Arc<AppState> {
    Arc::new(AppState {
        session_manager: SessionManagerHandle::test_handle(),
        broadcast_tx: tokio::sync::broadcast::channel(8).0,
        provider_registry_snapshot: Arc::new(RwLock::new(ProviderRegistry::default())),
        provider_info_snapshot: Arc::new(RwLock::new(ProviderInfoSnapshot {
            name: "bootstrap".into(),
            model: "bootstrap".into(),
            connected: true,
        })),
        channel_registry_path: aia_config::default_channels_path(),
        channel_registry_snapshot: Arc::new(RwLock::new(ChannelRegistry::default())),
        store: Arc::new(agent_store::AiaStore::in_memory().expect("memory store")),
    })
}

fn seed_trace(state: &AppState, id: &str, started_at_ms: u64) {
    seed_trace_with_request_kind(state, id, started_at_ms, "completion");
}

fn seed_trace_with_request_kind(
    state: &AppState,
    id: &str,
    started_at_ms: u64,
    request_kind: &str,
) {
    let loop_id = id.split("-step-").next().unwrap_or(id);
    state
        .store
        .record(&LlmTraceRecord {
            id: id.into(),
            trace_id: format!("trace-loop-{loop_id}"),
            span_id: format!("span-{id}"),
            parent_span_id: None,
            root_span_id: format!("root-{loop_id}"),
            operation_name: "responses.create".into(),
            span_kind: LlmTraceSpanKind::Client,
            turn_id: format!("turn-{loop_id}"),
            run_id: format!("run-{loop_id}"),
            request_kind: request_kind.into(),
            step_index: if id.contains("step-1") { 1 } else { 0 },
            provider: "openai".into(),
            protocol: "responses".into(),
            model: "gpt-5.4".into(),
            base_url: "https://api.openai.com".into(),
            endpoint_path: "/v1/responses".into(),
            streaming: true,
            started_at_ms,
            finished_at_ms: Some(started_at_ms + 25),
            duration_ms: Some(25),
            status_code: Some(200),
            status: LlmTraceStatus::Succeeded,
            stop_reason: Some("completed".into()),
            error: None,
            request_summary: serde_json::json!({
                "user_message": if request_kind == "compression" { serde_json::Value::Null } else { serde_json::json!("hi") }
            }),
            provider_request: serde_json::json!({ "model": "gpt-5.4" }),
            response_summary: serde_json::json!({ "output_text": "hello" }),
            response_body: Some("{\"ok\":true}".into()),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cached_tokens: Some(2),
            otel_attributes: serde_json::json!({ "http.method": "POST" }),
            events: vec![LlmTraceEvent {
                name: "response.completed".into(),
                at_ms: started_at_ms + 25,
                attributes: serde_json::json!({}),
            }],
        })
        .expect("trace record should save");
}

#[test]
fn model_config_dto_round_trip_preserves_limit() {
    let dto = ModelConfigDto {
        id: "gpt-4.1".into(),
        display_name: Some("GPT-4.1".into()),
        limit: Some(ModelLimitDto { context: Some(200_000), output: Some(131_072) }),
        default_temperature: Some(0.2),
        supports_reasoning: true,
        reasoning_effort: Some("medium".into()),
    };

    let model = ModelConfig::from(dto.clone());
    assert_eq!(model.limit, Some(ModelLimit { context: Some(200_000), output: Some(131_072) }));

    let round_trip = ModelConfigDto::from(&model);
    assert_eq!(round_trip.limit, dto.limit);
}

#[test]
fn json_response_serializes_payload() {
    let (status, body) = json_response(StatusCode::CREATED, serde_json::json!({ "ok": true }));

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body.0, serde_json::json!({ "ok": true }));
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_session_id_prefers_explicit_id() {
    let state = test_state();

    let resolved = resolve_session_id(state.as_ref(), Some("session-explicit".into()))
        .await
        .expect("explicit session id should resolve");

    assert_eq!(resolved.as_deref(), Some("session-explicit"));
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_session_id_falls_back_to_first_stored_session() {
    let state = test_state();
    state
        .store
        .create_session(&agent_store::SessionRecord {
            id: "session-1".into(),
            title: "First".into(),
            created_at: "2026-03-16T00:00:00Z".into(),
            updated_at: "2026-03-16T00:00:00Z".into(),
            model: "bootstrap".into(),
        })
        .expect("first session should save");
    state
        .store
        .create_session(&agent_store::SessionRecord {
            id: "session-2".into(),
            title: "Second".into(),
            created_at: "2026-03-16T00:00:01Z".into(),
            updated_at: "2026-03-16T00:00:01Z".into(),
            model: "bootstrap".into(),
        })
        .expect("second session should save");

    let resolved = resolve_session_id(state.as_ref(), None)
        .await
        .expect("fallback session lookup should succeed");

    assert_eq!(resolved.as_deref(), Some("session-1"));
}

#[test]
fn cancel_turn_request_deserializes_session_id() {
    let parsed: CancelTurnRequest = serde_json::from_value(serde_json::json!({
        "session_id": "session-1"
    }))
    .expect("cancel turn request should deserialize");

    assert_eq!(parsed.session_id.as_deref(), Some("session-1"));
}

#[test]
fn create_channel_request_deserializes_feishu_payload() {
    let parsed: CreateChannelRequest = serde_json::from_value(serde_json::json!({
        "id": "default",
        "name": "默认飞书",
        "transport": "feishu",
        "enabled": true,
        "app_id": "cli_xxx",
        "app_secret": "secret",
        "base_url": "https://open.feishu.cn",
        "require_mention": true,
        "thread_mode": true
    }))
    .expect("create channel request should deserialize");

    assert_eq!(parsed.id, "default");
    assert_eq!(parsed.transport, "feishu");
    assert!(parsed.thread_mode);
}

#[test]
fn update_channel_request_allows_partial_secret_update() {
    let parsed: UpdateChannelRequest = serde_json::from_value(serde_json::json!({
        "enabled": false,
        "app_secret": ""
    }))
    .expect("update channel request should deserialize");

    assert_eq!(parsed.enabled, Some(false));
    assert_eq!(parsed.app_secret.as_deref(), Some(""));
}

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
