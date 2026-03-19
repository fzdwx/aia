use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime_worker::RunningTurnHandle;
use crate::sse::TurnStatus;

use super::{
    CurrentTurnSnapshot, SessionManagerConfig, SessionQueryService, SessionSlot, SlotStatus,
    collect_runtime_events, read_lock, spawn_session_manager, update_current_turn_status,
    write_lock,
};

fn run_async<T>(future: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build");
    runtime.block_on(future)
}

fn poison_lock<T>(lock: &RwLock<T>) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let _guard = lock.write().expect("test should acquire write lock before poisoning");
        panic!("poison test lock");
    }));
}

fn temp_session_path(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-session-manager-{name}-{suffix}.jsonl"))
}

fn temp_session_dir(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-session-manager-{name}-{suffix}"))
}

#[test]
fn recovered_read_lock_returns_inner_value_after_poison() {
    let lock = RwLock::new(vec![1, 2, 3]);
    poison_lock(&lock);

    let guard = read_lock(&lock);
    assert_eq!(&*guard, &[1, 2, 3]);
}

#[test]
fn recovered_write_lock_allows_mutation_after_poison() {
    let lock = RwLock::new(vec![1, 2, 3]);
    poison_lock(&lock);

    write_lock(&lock).push(4);
    assert_eq!(&*read_lock(&lock), &[1, 2, 3, 4]);
}

#[test]
fn get_session_info_uses_cached_stats_when_turn_is_running() {
    let mut slots = std::collections::HashMap::new();
    slots.insert(
        "session-1".to_string(),
        SessionSlot {
            runtime: None,
            subscriber: 0,
            session_path: temp_session_path("context-stats-missing"),
            history: Arc::new(RwLock::new(Vec::new())),
            current_turn: Arc::new(RwLock::new(None)),
            context_stats: Arc::new(RwLock::new(agent_runtime::ContextStats {
                total_entries: 3,
                anchor_count: 1,
                entries_since_last_anchor: 1,
                last_input_tokens: Some(42),
                context_limit: Some(1024),
                output_limit: Some(256),
                pressure_ratio: Some(42.0 / 1024.0),
            })),
            running_turn: None,
            pending_provider_binding: None,
            status: SlotStatus::Running,
        },
    );

    let query = SessionQueryService::new(&mut slots);
    let stats = query.session_info("session-1").expect("session info should use cache");

    assert_eq!(stats.total_entries, 3);
    assert_eq!(stats.anchor_count, 1);
    assert_eq!(stats.entries_since_last_anchor, 1);
    assert_eq!(stats.last_input_tokens, Some(42));
    assert_eq!(stats.context_limit, Some(1024));
    assert_eq!(stats.output_limit, Some(256));
    assert_eq!(stats.pressure_ratio, Some(42.0 / 1024.0));
}

#[test]
fn collect_runtime_events_reports_missing_subscriber() {
    let path = temp_session_path("missing-subscriber");
    let store = Arc::new(agent_store::AiaStore::new(":memory:").expect("memory store"));
    let (identity, model) =
        super::build_model_from_selection(super::ProviderLaunchChoice::Bootstrap, Some(store))
            .expect("bootstrap model");
    let mut runtime =
        agent_runtime::AgentRuntime::new(model, builtin_tools::build_tool_registry(), identity);

    let error =
        collect_runtime_events(&mut runtime, 999).expect_err("missing subscriber should fail");

    assert_eq!(error.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    assert!(error.message.contains("runtime event collection failed"));
    assert!(error.message.contains("订阅者不存在"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_current_turn_status_recovers_from_poisoned_snapshot_lock() {
    let snapshot = Arc::new(RwLock::new(Some(CurrentTurnSnapshot {
        turn_id: "turn-1".into(),
        started_at_ms: 1,
        user_message: "hello".into(),
        status: TurnStatus::Waiting,
        blocks: Vec::new(),
    })));
    poison_lock(&snapshot);

    update_current_turn_status(&snapshot, TurnStatus::Generating);

    let guard = read_lock(&snapshot);
    let current = guard.as_ref().expect("snapshot should still exist");
    assert_eq!(current.status, TurnStatus::Generating);
}

#[test]
fn handle_cancel_turn_marks_running_snapshot_as_cancelled() {
    let current_turn = Arc::new(RwLock::new(Some(CurrentTurnSnapshot {
        turn_id: "turn-1".into(),
        started_at_ms: 1,
        user_message: "hello".into(),
        status: TurnStatus::Working,
        blocks: Vec::new(),
    })));
    let control = agent_runtime::TurnControl::new(agent_core::AbortSignal::new());
    let handle = RunningTurnHandle { control: control.clone() };
    let mut slots = std::collections::HashMap::new();
    slots.insert(
        "session-1".to_string(),
        SessionSlot {
            runtime: None,
            subscriber: 0,
            session_path: std::path::PathBuf::new(),
            history: Arc::new(RwLock::new(Vec::new())),
            current_turn: current_turn.clone(),
            context_stats: Arc::new(RwLock::new(agent_runtime::ContextStats {
                total_entries: 0,
                anchor_count: 0,
                entries_since_last_anchor: 0,
                last_input_tokens: None,
                context_limit: None,
                output_limit: None,
                pressure_ratio: None,
            })),
            running_turn: Some(handle),
            pending_provider_binding: None,
            status: SlotStatus::Running,
        },
    );
    let (broadcast_tx, _broadcast_rx) = tokio::sync::broadcast::channel(8);
    let _config = SessionManagerConfig {
        sessions_dir: std::path::PathBuf::new(),
        store: Arc::new(agent_store::AiaStore::new(":memory:").expect("memory store")),
        registry: provider_registry::ProviderRegistry::default(),
        store_path: std::path::PathBuf::new(),
        broadcast_tx,
        provider_registry_snapshot: Arc::new(RwLock::new(
            provider_registry::ProviderRegistry::default(),
        )),
        provider_info_snapshot: Arc::new(RwLock::new(super::ProviderInfoSnapshot {
            name: "bootstrap".into(),
            model: "bootstrap".into(),
            connected: true,
        })),
        workspace_root: std::path::PathBuf::new(),
        user_agent: "test-agent".into(),
    };

    let mut query = SessionQueryService::new(&mut slots);
    let cancelled = query.cancel_turn("session-1").expect("cancel succeeds");

    assert!(cancelled);
    assert!(control.abort_signal().is_aborted());
    let guard = read_lock(&current_turn);
    let current = guard.as_ref().expect("snapshot should still exist");
    assert_eq!(current.status, TurnStatus::Cancelled);
}

#[test]
fn spawned_turn_worker_completes_bootstrap_turn() {
    let temp_root = temp_session_dir("turn-worker");
    let cleanup_root = temp_root.clone();
    let sessions_dir = temp_root.join("sessions");
    std::fs::create_dir_all(&sessions_dir).expect("sessions dir should exist");
    let store = Arc::new(
        agent_store::AiaStore::new(temp_root.join("store.sqlite3"))
            .expect("sqlite store should initialize"),
    );
    let registry = provider_registry::ProviderRegistry::default();
    let (broadcast_tx, _broadcast_rx) = tokio::sync::broadcast::channel(32);

    run_async(async {
        let handle = spawn_session_manager(SessionManagerConfig {
            sessions_dir,
            store,
            registry: registry.clone(),
            store_path: temp_root.join("providers.json"),
            broadcast_tx,
            provider_registry_snapshot: Arc::new(RwLock::new(registry)),
            provider_info_snapshot: Arc::new(RwLock::new(super::ProviderInfoSnapshot {
                name: "bootstrap".into(),
                model: "bootstrap".into(),
                connected: true,
            })),
            workspace_root: temp_root.clone(),
            user_agent: "test-agent".into(),
        });

        let session = handle
            .create_session(Some("Async worker".into()))
            .await
            .expect("session should be created");
        let accepted_turn_id = handle
            .submit_turn(session.id.clone(), "hello from async worker".into())
            .await
            .expect("turn should be accepted");

        let mut completed_turn = None;
        for _ in 0..200 {
            let history =
                handle.get_history(session.id.clone()).await.expect("history should be readable");
            if let Some(history_turn) = history.last().cloned() {
                completed_turn = Some(history_turn);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let turn = completed_turn.expect("turn should complete");

        assert!(accepted_turn_id.starts_with("srv-turn-"));
        assert_eq!(turn.user_message, "hello from async worker");
        assert_eq!(turn.outcome, agent_runtime::TurnOutcome::Succeeded);
        assert!(
            turn.assistant_message
                .as_deref()
                .is_some_and(|text: &str| text.contains("Bootstrap 模式收到"))
        );

        let current = handle
            .get_current_turn(session.id.clone())
            .await
            .expect("current turn should be readable");
        assert!(current.is_none());
    });

    let _ = std::fs::remove_dir_all(cleanup_root);
}
