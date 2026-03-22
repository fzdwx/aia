use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime_worker::RunningTurnHandle;
use crate::sse::TurnStatus;
use agent_core::RequestTimeoutConfig;

use super::{
    CurrentTurnSnapshot, SessionManagerConfig, SessionQueryService, SessionSlot,
    SessionSlotFactory, SlotExecutionState, SlotStatus, collect_runtime_events, read_lock,
    spawn_session_manager, update_current_turn_status, write_lock,
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

fn sample_manager_config(root: &std::path::Path) -> SessionManagerConfig {
    let sessions_dir = root.join("sessions");
    std::fs::create_dir_all(&sessions_dir).expect("sessions dir should exist");
    let store = Arc::new(
        agent_store::AiaStore::new(root.join("store.sqlite3"))
            .expect("sqlite store should initialize"),
    );
    let registry = provider_registry::ProviderRegistry::default();
    let (broadcast_tx, _broadcast_rx) = tokio::sync::broadcast::channel(8);

    SessionManagerConfig {
        sessions_dir,
        store,
        registry: registry.clone(),
        broadcast_tx,
        provider_registry_snapshot: Arc::new(RwLock::new(registry)),
        provider_info_snapshot: Arc::new(RwLock::new(super::ProviderInfoSnapshot {
            name: "bootstrap".into(),
            model: "bootstrap".into(),
            connected: true,
        })),
        workspace_root: root.to_path_buf(),
        user_agent: "test-agent".into(),
        request_timeout: RequestTimeoutConfig {
            read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
        },
        system_prompt: None,
        runtime_hooks: agent_runtime::RuntimeHooks::default(),
    }
}

fn build_idle_slot(name: &str) -> SessionSlot {
    let root = temp_session_dir(name);
    let config = sample_manager_config(&root);
    SessionSlotFactory::new(&config).create("session-1").expect("session slot should build")
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
            session_path: temp_session_path("context-stats-missing"),
            provider_binding: session_tape::SessionProviderBinding::Bootstrap,
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
            execution: SlotExecutionState::Running {
                subscriber: 0,
                running_turn: RunningTurnHandle {
                    control: agent_runtime::TurnControl::new(agent_core::AbortSignal::new()),
                },
                pending_provider_binding: None,
            },
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
fn running_session_slot_keeps_in_memory_provider_binding() {
    let mut slots = std::collections::HashMap::new();
    slots.insert(
        "session-1".to_string(),
        SessionSlot {
            session_path: temp_session_path("running-session-settings"),
            provider_binding: session_tape::SessionProviderBinding::Provider {
                name: "primary".into(),
                model: "model-primary".into(),
                base_url: "https://primary.example.com".into(),
                protocol: "openai-responses".into(),
                reasoning_effort: Some("high".into()),
            },
            history: Arc::new(RwLock::new(Vec::new())),
            current_turn: Arc::new(RwLock::new(None)),
            context_stats: Arc::new(RwLock::new(agent_runtime::ContextStats {
                total_entries: 0,
                anchor_count: 0,
                entries_since_last_anchor: 0,
                last_input_tokens: None,
                context_limit: None,
                output_limit: None,
                pressure_ratio: None,
            })),
            execution: SlotExecutionState::Running {
                subscriber: 0,
                running_turn: RunningTurnHandle {
                    control: agent_runtime::TurnControl::new(agent_core::AbortSignal::new()),
                },
                pending_provider_binding: None,
            },
        },
    );

    let slot = slots.get("session-1").expect("session slot should exist");
    assert!(slot.runtime().is_none());
    assert_eq!(slot.status(), SlotStatus::Running);
    assert!(matches!(
        &slot.provider_binding,
        session_tape::SessionProviderBinding::Provider {
            name,
            model,
            reasoning_effort: Some(effort),
            ..
        } if name == "primary" && model == "model-primary" && effort == "high"
    ));
}

#[test]
fn session_slot_begin_turn_transitions_to_running_state() {
    let mut slot = build_idle_slot("slot-begin-turn");

    let (_runtime, _subscriber, running_turn) =
        slot.begin_turn().expect("idle slot should start turn");

    assert_eq!(slot.status(), SlotStatus::Running);
    assert!(slot.runtime().is_none());
    assert!(slot.running_turn().is_some());
    assert!(!running_turn.control.abort_signal().is_aborted());
}

#[test]
fn session_slot_finish_turn_restores_idle_state_and_clears_pending_binding() {
    let mut slot = build_idle_slot("slot-finish-turn");
    let (runtime, subscriber, _running_turn) =
        slot.begin_turn().expect("idle slot should start turn");

    slot.replace_pending_provider_binding(Some(session_tape::SessionProviderBinding::Provider {
        name: "primary".into(),
        model: "model-primary".into(),
        base_url: "https://primary.example.com".into(),
        protocol: "openai-responses".into(),
        reasoning_effort: None,
    }))
    .expect("running slot should accept pending binding");

    slot.finish_turn(runtime, subscriber).expect("running slot should finish turn");

    assert_eq!(slot.status(), SlotStatus::Idle);
    assert!(slot.runtime().is_some());
    assert!(slot.running_turn().is_none());
    assert!(slot.pending_provider_binding().is_none());
}

#[test]
fn session_slot_factory_fails_on_malformed_latest_provider_binding() {
    let root = temp_session_dir("slot-factory-malformed-binding");
    let config = sample_manager_config(&root);
    let session_path = config.sessions_dir.join("session-1.jsonl");
    std::fs::write(
        &session_path,
        concat!(
            "{\"id\":1,\"kind\":\"event\",\"payload\":{\"name\":\"provider_binding\",\"data\":{\"name\":\"older\",\"model\":\"gpt-4.1-mini\",\"base_url\":\"https://api.openai.com/v1\",\"protocol\":\"openai-responses\"}},\"meta\":{},\"date\":\"2026-03-21T00:00:00Z\"}\n",
            "{\"id\":2,\"kind\":\"event\",\"payload\":{\"name\":\"provider_binding\",\"data\":{\"broken\":true}},\"meta\":{},\"date\":\"2026-03-21T00:00:01Z\"}\n"
        ),
    )
    .expect("broken session tape should be written");

    let error = SessionSlotFactory::new(&config)
        .create("session-1")
        .err()
        .expect("malformed latest binding should fail slot recovery");

    assert!(error.message.contains("provider_binding"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn get_session_settings_reports_recovery_error_for_malformed_latest_provider_binding() {
    let temp_root = temp_session_dir("settings-malformed-binding");
    let cleanup_root = temp_root.clone();
    let config = sample_manager_config(&temp_root);
    config
        .store
        .create_session(&agent_store::SessionRecord::new(
            "session-1",
            "Broken Session",
            "stale-store-model",
        ))
        .expect("broken session should exist in store");
    std::fs::write(
        config.sessions_dir.join("session-1.jsonl"),
        concat!(
            "{\"id\":1,\"kind\":\"event\",\"payload\":{\"name\":\"provider_binding\",\"data\":{\"name\":\"older\",\"model\":\"gpt-4.1-mini\",\"base_url\":\"https://api.openai.com/v1\",\"protocol\":\"openai-responses\"}},\"meta\":{},\"date\":\"2026-03-21T00:00:00Z\"}\n",
            "{\"id\":2,\"kind\":\"event\",\"payload\":{\"name\":\"provider_binding\",\"data\":{\"broken\":true}},\"meta\":{},\"date\":\"2026-03-21T00:00:01Z\"}\n"
        ),
    )
    .expect("broken session tape should be written");

    run_async(async {
        let handle = spawn_session_manager(config);
        let error = handle
            .get_session_settings("session-1".into())
            .await
            .expect_err("malformed latest binding should surface recovery error");

        assert!(error.message.contains("provider_binding"));
    });

    let _ = std::fs::remove_dir_all(cleanup_root);
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
            session_path: std::path::PathBuf::new(),
            provider_binding: session_tape::SessionProviderBinding::Bootstrap,
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
            execution: SlotExecutionState::Running {
                subscriber: 0,
                running_turn: handle,
                pending_provider_binding: None,
            },
        },
    );
    let (broadcast_tx, _broadcast_rx) = tokio::sync::broadcast::channel(8);
    let _config = SessionManagerConfig {
        sessions_dir: std::path::PathBuf::new(),
        store: Arc::new(agent_store::AiaStore::new(":memory:").expect("memory store")),
        registry: provider_registry::ProviderRegistry::default(),
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
        request_timeout: RequestTimeoutConfig {
            read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
        },
        system_prompt: None,
        runtime_hooks: agent_runtime::RuntimeHooks::default(),
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
            broadcast_tx,
            provider_registry_snapshot: Arc::new(RwLock::new(registry)),
            provider_info_snapshot: Arc::new(RwLock::new(super::ProviderInfoSnapshot {
                name: "bootstrap".into(),
                model: "bootstrap".into(),
                connected: true,
            })),
            workspace_root: temp_root.clone(),
            user_agent: "test-agent".into(),
            request_timeout: RequestTimeoutConfig {
                read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
            },
            system_prompt: None,
            runtime_hooks: agent_runtime::RuntimeHooks::default(),
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

#[test]
fn spawned_turn_worker_applies_custom_system_prompt_and_runtime_hooks() {
    let temp_root = temp_session_dir("turn-worker-hooks");
    let cleanup_root = temp_root.clone();
    let sessions_dir = temp_root.join("sessions");
    std::fs::create_dir_all(&sessions_dir).expect("sessions dir should exist");
    let store = Arc::new(
        agent_store::AiaStore::new(temp_root.join("store.sqlite3"))
            .expect("sqlite store should initialize"),
    );
    let registry = provider_registry::ProviderRegistry::default();
    let (broadcast_tx, _broadcast_rx) = tokio::sync::broadcast::channel(32);
    let seen_instructions = Arc::new(Mutex::new(Vec::<String>::new()));

    run_async(async {
        let runtime_hooks = agent_runtime::RuntimeHooks::default().on_before_provider_request({
            let seen_instructions = seen_instructions.clone();
            move |event| {
                if let Some(instructions) = event.request.instructions.clone() {
                    seen_instructions.lock().expect("test mutex should lock").push(instructions);
                }
                Ok(())
            }
        });

        let handle = spawn_session_manager(SessionManagerConfig {
            sessions_dir,
            store,
            registry: registry.clone(),
            broadcast_tx,
            provider_registry_snapshot: Arc::new(RwLock::new(registry)),
            provider_info_snapshot: Arc::new(RwLock::new(super::ProviderInfoSnapshot {
                name: "bootstrap".into(),
                model: "bootstrap".into(),
                connected: true,
            })),
            workspace_root: temp_root.clone(),
            user_agent: "test-agent".into(),
            request_timeout: RequestTimeoutConfig {
                read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
            },
            system_prompt: Some("你是测试客户端代理。".into()),
            runtime_hooks,
        });

        let session = handle
            .create_session(Some("Prompt hook".into()))
            .await
            .expect("session should be created");
        let _ = handle
            .submit_turn(session.id.clone(), "hello".into())
            .await
            .expect("turn should be accepted");

        for _ in 0..200 {
            let history =
                handle.get_history(session.id.clone()).await.expect("history should be readable");
            if !history.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    });

    let seen_instructions = seen_instructions.lock().expect("test mutex should lock");
    assert!(!seen_instructions.is_empty());
    assert!(seen_instructions[0].contains("你是测试客户端代理。"));
    assert!(!seen_instructions[0].contains("Context Contract"));

    let _ = std::fs::remove_dir_all(cleanup_root);
}
