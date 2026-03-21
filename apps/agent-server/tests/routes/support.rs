use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::RequestTimeoutConfig;
use agent_store::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore};
use channel_bridge::ChannelProfileRegistry;
use provider_registry::ProviderRegistry;

use crate::{
    channel_host::{build_channel_adapter_catalog, build_channel_runtime},
    session_manager::{
        ProviderInfoSnapshot, SessionManagerConfig, SessionManagerHandle, spawn_session_manager,
    },
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

fn temp_root(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-routes-{name}-{suffix}"))
}

pub(crate) fn test_state_with_session_manager(
    name: &str,
    registry: ProviderRegistry,
) -> (Arc<AppState>, std::path::PathBuf) {
    let root = temp_root(name);
    test_state_with_session_manager_setup(root, registry, |_, _| {})
}

pub(crate) fn test_state_with_session_manager_setup(
    root: std::path::PathBuf,
    registry: ProviderRegistry,
    setup: impl FnOnce(&std::path::Path, &Arc<agent_store::AiaStore>),
) -> (Arc<AppState>, std::path::PathBuf) {
    std::fs::create_dir_all(root.join("sessions")).expect("sessions dir should exist");

    let store = Arc::new(
        agent_store::AiaStore::new(root.join("store.sqlite3"))
            .expect("sqlite store should initialize"),
    );
    setup(root.as_path(), &store);
    let active_provider = registry.active_provider().cloned();
    let provider_registry_snapshot = Arc::new(RwLock::new(registry.clone()));
    let provider_info_snapshot = Arc::new(RwLock::new(match active_provider.as_ref() {
        Some(profile) => ProviderInfoSnapshot {
            name: profile.name.clone(),
            model: profile.default_model_id().unwrap_or("").to_string(),
            connected: true,
        },
        None => ProviderInfoSnapshot {
            name: "bootstrap".into(),
            model: "bootstrap".into(),
            connected: true,
        },
    }));
    let broadcast_tx = tokio::sync::broadcast::channel(8).0;
    let session_manager = spawn_session_manager(SessionManagerConfig {
        sessions_dir: root.join("sessions"),
        store: store.clone(),
        registry: registry.clone(),
        broadcast_tx: broadcast_tx.clone(),
        provider_registry_snapshot: provider_registry_snapshot.clone(),
        provider_info_snapshot: provider_info_snapshot.clone(),
        workspace_root: root.clone(),
        user_agent: "test-agent".into(),
        request_timeout: RequestTimeoutConfig {
            read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
        },
        system_prompt: agent_prompts::SystemPromptConfig::default(),
        runtime_hooks: agent_runtime::RuntimeHooks::default(),
    });
    let channel_adapter_catalog = Arc::new(build_channel_adapter_catalog(
        store.clone(),
        session_manager.clone(),
        broadcast_tx.clone(),
    ));

    (
        Arc::new(AppState {
            session_manager,
            broadcast_tx,
            provider_registry_snapshot,
            provider_info_snapshot,
            channel_profile_registry_snapshot: Arc::new(RwLock::new(
                ChannelProfileRegistry::default(),
            )),
            channel_mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
            store,
            channel_adapter_catalog: channel_adapter_catalog.clone(),
            channel_runtime: Arc::new(tokio::sync::Mutex::new(build_channel_runtime(
                channel_adapter_catalog.as_ref().clone(),
            ))),
        }),
        root,
    )
}

pub(crate) fn seed_trace(state: &AppState, id: &str, started_at_ms: u64) {
    seed_trace_with_request_kind_and_status(
        state,
        id,
        started_at_ms,
        "completion",
        LlmTraceStatus::Succeeded,
    );
}

pub(crate) fn seed_trace_with_request_kind(
    state: &AppState,
    id: &str,
    started_at_ms: u64,
    request_kind: &str,
) {
    seed_trace_with_request_kind_and_status(
        state,
        id,
        started_at_ms,
        request_kind,
        LlmTraceStatus::Succeeded,
    );
}

pub(crate) fn seed_trace_with_request_kind_and_status(
    state: &AppState,
    id: &str,
    started_at_ms: u64,
    request_kind: &str,
    status: LlmTraceStatus,
) {
    let loop_id = id.split("-step-").next().unwrap_or(id);
    let is_failed = status == LlmTraceStatus::Failed;
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
            session_id: Some(format!("session-{loop_id}")),
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
            status_code: if is_failed { Some(500) } else { Some(200) },
            status,
            stop_reason: if is_failed {
                Some("error".into())
            } else {
                Some("completed".into())
            },
            error: if is_failed {
                Some("request failed".into())
            } else {
                None
            },
            request_summary: serde_json::json!({
                "user_message": if request_kind == "compression" { serde_json::Value::Null } else { serde_json::json!("hi") }
            }),
            provider_request: serde_json::json!({ "model": "gpt-5.4" }),
            response_summary: serde_json::json!({ "output_text": "hello" }),
            response_body: Some(
                if is_failed {
                    "{\"ok\":false}".into()
                } else {
                    "{\"ok\":true}".into()
                }
            ),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cached_tokens: Some(2),
            otel_attributes: serde_json::json!({ "http.method": "POST" }),
            events: vec![LlmTraceEvent {
                name: if is_failed {
                    "response.failed".into()
                } else {
                    "response.completed".into()
                },
                at_ms: started_at_ms + 25,
                attributes: serde_json::json!({}),
            }],
        })
        .expect("trace record should save");
}

pub(crate) fn seed_tool_trace_with_changes(
    state: &AppState,
    loop_id: &str,
    started_at_ms: u64,
    tool_name: &str,
    added: u64,
    removed: u64,
) {
    state
        .store
        .record(&LlmTraceRecord {
            id: format!("{loop_id}-tool-{started_at_ms}"),
            trace_id: format!("trace-loop-{loop_id}"),
            span_id: format!("span-{loop_id}-tool-{started_at_ms}"),
            parent_span_id: Some(format!("span-{loop_id}-parent")),
            root_span_id: format!("root-{loop_id}"),
            operation_name: "execute_tool".into(),
            span_kind: LlmTraceSpanKind::Internal,
            session_id: Some(format!("session-{loop_id}")),
            turn_id: format!("turn-{loop_id}"),
            run_id: format!("run-{loop_id}"),
            request_kind: "tool".into(),
            step_index: 9,
            provider: "builtin".into(),
            protocol: "local".into(),
            model: tool_name.into(),
            base_url: "local".into(),
            endpoint_path: format!("/{tool_name}"),
            streaming: false,
            started_at_ms,
            finished_at_ms: Some(started_at_ms + 10),
            duration_ms: Some(10),
            status_code: None,
            status: LlmTraceStatus::Succeeded,
            stop_reason: None,
            error: None,
            request_summary: serde_json::json!({}),
            provider_request: serde_json::json!({}),
            response_summary: serde_json::json!({}),
            response_body: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cached_tokens: None,
            otel_attributes: serde_json::json!({
                "aia.tool.details": {
                    "added": added,
                    "removed": removed,
                }
            }),
            events: vec![],
        })
        .expect("tool trace should save");
}
