use std::sync::{Arc, RwLock};

use agent_store::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore};
use channel_bridge::ChannelProfileRegistry;
use provider_registry::ProviderRegistry;

use crate::{
    channel_host::{build_channel_adapter_catalog, build_channel_runtime},
    session_manager::{ProviderInfoSnapshot, SessionManagerHandle},
    state::AppState,
};

pub(crate) fn test_state() -> Arc<AppState> {
    let session_manager = SessionManagerHandle::test_handle();
    let broadcast_tx = tokio::sync::broadcast::channel(8).0;
    let store = Arc::new(agent_store::AiaStore::in_memory().expect("memory store"));
    let provider_registry_snapshot = Arc::new(RwLock::new(ProviderRegistry::default()));
    let provider_info_snapshot = Arc::new(RwLock::new(ProviderInfoSnapshot {
        name: "bootstrap".into(),
        model: "bootstrap".into(),
        connected: true,
    }));
    let channel_adapter_catalog = Arc::new(build_channel_adapter_catalog(
        store.clone(),
        session_manager.clone(),
        broadcast_tx.clone(),
    ));
    Arc::new(AppState {
        session_manager: session_manager.clone(),
        broadcast_tx: broadcast_tx.clone(),
        provider_registry_snapshot,
        provider_info_snapshot,
        channel_profile_registry_snapshot: Arc::new(RwLock::new(ChannelProfileRegistry::default())),
        channel_mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
        store: store.clone(),
        channel_adapter_catalog: channel_adapter_catalog.clone(),
        channel_runtime: Arc::new(tokio::sync::Mutex::new(build_channel_runtime(
            channel_adapter_catalog.as_ref().clone(),
        ))),
    })
}

pub(crate) fn seed_trace(state: &AppState, id: &str, started_at_ms: u64) {
    seed_trace_with_request_kind(state, id, started_at_ms, "completion");
}

pub(crate) fn seed_trace_with_request_kind(
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
