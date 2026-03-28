use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use provider_registry::{ProviderProfile, ProviderRegistry};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{ResolvePendingQuestionRequest, UpdateSessionSettingsRequest, handlers};
use crate::routes::session::SessionQuery;
use crate::routes::test_support::{
    test_state_with_session_manager, test_state_with_session_manager_setup,
};
use agent_core::{
    QuestionAnswer, QuestionItem, QuestionKind, QuestionRequest, QuestionResult,
    QuestionResultStatus,
};
use agent_store::{SessionAutoRenamePolicy, SessionTitleSource};

fn sample_registry() -> ProviderRegistry {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile::openai_responses(
        "primary",
        "https://primary.example.com",
        "primary-key",
        "model-primary",
    ));
    registry.upsert(ProviderProfile::openai_responses(
        "backup",
        "https://backup.example.com",
        "backup-key",
        "model-backup",
    ));
    registry
}

async fn response_body_json(response: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

fn sample_question_request() -> QuestionRequest {
    QuestionRequest {
        request_id: "qreq_123".into(),
        invocation_id: "call_123".into(),
        turn_id: "turn_123".into(),
        questions: vec![QuestionItem {
            id: "database".into(),
            question: "Use which database?".into(),
            kind: QuestionKind::Choice,
            required: true,
            multi_select: false,
            options: Vec::new(),
            placeholder: None,
            recommended_option_id: None,
            recommendation_reason: None,
        }],
    }
}

#[tokio::test(flavor = "current_thread")]
async fn session_list_projection_uses_session_manager_model_not_store() {
    let (state, root) =
        test_state_with_session_manager("session-list-projection", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    state
        .store
        .update_session_async(session.id.clone(), None, Some("bootstrap".into()))
        .await
        .expect("session record should be overwritten for regression setup");

    let (status, body) = handlers::list_sessions(State(state.clone())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.0.as_array().and_then(|items| items.first()).and_then(|item| item.get("model")),
        Some(&serde_json::json!("model-primary"))
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn update_session_settings_does_not_persist_session_record_model() {
    let (state, root) =
        test_state_with_session_manager("session-settings-persist", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let response = handlers::update_session_settings(
        State(state.clone()),
        Json(UpdateSessionSettingsRequest {
            session_id: Some(session.id.clone()),
            provider: Some("backup".into()),
            model: Some("model-backup".into()),
            reasoning_effort: None,
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response).await;
    assert_eq!(body.get("model"), Some(&serde_json::json!("model-backup")));

    let stored = state
        .store
        .get_session_async(session.id)
        .await
        .expect("session lookup should succeed")
        .expect("session should remain in store");
    assert_eq!(stored.model, "model-primary");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn session_list_projection_marks_missing_slot_model_unavailable() {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let root = std::env::temp_dir().join(format!("aia-routes-session-list-missing-slot-{suffix}"));
    let (state, root) =
        test_state_with_session_manager_setup(root, sample_registry(), |root, store| {
            store
                .create_session(&agent_store::SessionRecord {
                    id: "session-missing-slot".into(),
                    title: "Session Missing Slot".into(),
                    title_source: SessionTitleSource::Manual,
                    auto_rename_policy: SessionAutoRenamePolicy::Enabled,
                    created_at: "2026-03-21T00:00:00Z".into(),
                    updated_at: "2026-03-21T00:00:00Z".into(),
                    last_active_at: "2026-03-21T00:00:00Z".into(),
                    model: "stale-store-model".into(),
                })
                .expect("session record should be inserted directly into store");
            std::fs::write(root.join("sessions").join("session-missing-slot.jsonl"), "not-json\n")
                .expect("broken session tape should be written");
        });

    let (status, body) = handlers::list_sessions(State(state.clone())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.0
            .as_array()
            .and_then(|items| items
                .iter()
                .find(|item| item.get("id") == Some(&serde_json::json!("session-missing-slot"))))
            .and_then(|item| item.get("model")),
        Some(&serde_json::json!("unavailable"))
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn session_info_includes_workspace_root() {
    let (state, root) =
        test_state_with_session_manager("session-info-workspace-root", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let response = handlers::get_session_info(
        State(state.clone()),
        axum::extract::Query(SessionQuery {
            session_id: Some(session.id),
            before_turn_id: None,
            limit: None,
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response).await;
    assert_eq!(body.get("workspace_root"), Some(&serde_json::json!(root.display().to_string())));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn get_pending_question_returns_request_from_session_tape() {
    let (state, root) =
        test_state_with_session_manager("session-pending-question", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let session_path = root.join("sessions").join(format!("{}.jsonl", session.id));
    let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("session tape should load");
    tape.record_question_requested(&sample_question_request());
    tape.save_jsonl(&session_path).expect("session tape should save");

    let response = handlers::get_pending_question(
        State(state.clone()),
        axum::extract::Query(SessionQuery {
            session_id: Some(session.id),
            before_turn_id: None,
            limit: None,
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response).await;
    assert_eq!(body.get("pending"), Some(&serde_json::json!(true)));
    assert_eq!(
        body.get("request").and_then(|value| value.get("request_id")),
        Some(&serde_json::json!("qreq_123"))
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn get_history_restores_session_after_question_waiting_restart() {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let root =
        std::env::temp_dir().join(format!("aia-routes-session-history-question-restart-{suffix}"));
    let session_id = "session-question-restart";
    let (state, root) =
        test_state_with_session_manager_setup(root, sample_registry(), |root, store| {
            store
                .create_session(&agent_store::SessionRecord::new(
                    session_id,
                    "Question Session",
                    "model-primary",
                ))
                .expect("session record should be inserted");

            let session_path = root.join("sessions").join(format!("{session_id}.jsonl"));
            let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
                .expect("session tape should load");
            let turn_id = "turn-question-restart";
            tape.append_entry(
                session_tape::TapeEntry::message(&agent_core::Message::new(
                    agent_core::Role::User,
                    "请帮我确认偏好",
                ))
                .with_run_id(turn_id),
            );
            tape.append_entry(
                session_tape::TapeEntry::event("turn_waiting_for_question", None)
                    .with_run_id(turn_id),
            );
            tape.record_question_requested(&sample_question_request());
            tape.save_jsonl(&session_path).expect("session tape should save");
        });

    let response = handlers::get_history(
        State(state.clone()),
        axum::extract::Query(SessionQuery {
            session_id: Some(session_id.to_string()),
            before_turn_id: None,
            limit: Some(1),
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response).await;
    let turns = body
        .get("turns")
        .and_then(|value| value.as_array())
        .expect("history response should include turns array");
    assert!(turns.len() <= 1);
    assert_eq!(turns[0].get("outcome"), Some(&serde_json::json!("waiting_for_question")));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn get_history_reports_hydration_error_for_unrestorable_session() {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let root =
        std::env::temp_dir().join(format!("aia-routes-session-history-hydration-error-{suffix}"));
    let session_id = "session-broken-history";
    let (state, root) = test_state_with_session_manager_setup(
        root,
        sample_registry(),
        |root, store| {
            store
                .create_session(&agent_store::SessionRecord::new(
                    session_id,
                    "Broken Session",
                    "model-primary",
                ))
                .expect("session record should be inserted");

            std::fs::write(
            root.join("sessions").join(format!("{session_id}.jsonl")),
            concat!(
                "{\"id\":1,\"kind\":\"event\",\"payload\":{\"name\":\"provider_binding\",\"data\":{\"name\":\"older\",\"model\":\"gpt-4.1-mini\",\"base_url\":\"https://api.openai.com/v1\",\"protocol\":\"openai-responses\"}},\"meta\":{},\"date\":\"2026-03-21T00:00:00Z\"}\n",
                "{\"id\":2,\"kind\":\"event\",\"payload\":{\"name\":\"provider_binding\",\"data\":{\"broken\":true}},\"meta\":{},\"date\":\"2026-03-21T00:00:01Z\"}\n"
            ),
        )
        .expect("broken tape should be written");
        },
    );

    let response = handlers::get_history(
        State(state.clone()),
        axum::extract::Query(SessionQuery {
            session_id: Some(session_id.to_string()),
            before_turn_id: None,
            limit: Some(1),
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response_body_json(response).await;
    assert!(
        body.get("error")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("provider_binding"))
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn get_history_repairs_non_contiguous_question_tape_ids_on_restart() {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let root = std::env::temp_dir().join(format!("aia-routes-session-history-id-repair-{suffix}"));
    let session_id = "session-question-id-repair";
    let (state, root) = test_state_with_session_manager_setup(
        root,
        sample_registry(),
        |root, store| {
            store
                .create_session(&agent_store::SessionRecord::new(
                    session_id,
                    "Question Session",
                    "model-primary",
                ))
                .expect("session record should be inserted");

            std::fs::write(
            root.join("sessions").join(format!("{session_id}.jsonl")),
            concat!(
                "{\"id\":1,\"kind\":\"message\",\"payload\":{\"role\":\"user\",\"content\":\"请帮我确认偏好\"},\"meta\":{\"run_id\":\"turn-question-repair\"},\"date\":\"2026-03-21T00:00:00Z\"}\n",
                "{\"id\":2,\"kind\":\"event\",\"payload\":{\"name\":\"turn_waiting_for_question\",\"data\":null},\"meta\":{\"run_id\":\"turn-question-repair\"},\"date\":\"2026-03-21T00:00:01Z\"}\n",
                "{\"id\":3,\"kind\":\"event\",\"payload\":{\"name\":\"question_requested\",\"data\":{\"request_id\":\"qreq_123\",\"invocation_id\":\"call_123\",\"turn_id\":\"turn-question-repair\",\"questions\":[{\"id\":\"database\",\"question\":\"Use which database?\",\"kind\":\"choice\",\"required\":true,\"multi_select\":false,\"options\":[],\"placeholder\":null,\"recommended_option_id\":null,\"recommendation_reason\":null}]}},\"meta\":{},\"date\":\"2026-03-21T00:00:02Z\"}\n",
                "{\"id\":3,\"kind\":\"tool_result\",\"payload\":{\"invocation_id\":\"call_123\",\"tool_name\":\"Question\",\"content\":\"{}\",\"details\":null},\"meta\":{\"run_id\":\"turn-question-repair\"},\"date\":\"2026-03-21T00:00:03Z\"}\n",
                "{\"id\":4,\"kind\":\"event\",\"payload\":{\"name\":\"turn_completed\",\"data\":null},\"meta\":{\"run_id\":\"turn-question-repair\"},\"date\":\"2026-03-21T00:00:04Z\"}\n"
            ),
        )
        .expect("broken tape should be written");
        },
    );

    let response = handlers::get_history(
        State(state.clone()),
        axum::extract::Query(SessionQuery {
            session_id: Some(session_id.to_string()),
            before_turn_id: None,
            limit: Some(1),
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response).await;
    let turns = body
        .get("turns")
        .and_then(|value| value.as_array())
        .expect("history response should include turns array");
    assert!(turns.len() <= 1);

    let repaired =
        std::fs::read_to_string(root.join("sessions").join(format!("{session_id}.jsonl")))
            .expect("repaired tape should be readable");
    let ids = repaired
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("persisted line should be valid json")
                .get("id")
                .and_then(|value| value.as_u64())
                .expect("persisted line should contain id")
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![1, 2, 3, 4, 5, 6]);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_pending_question_appends_resolution_and_clears_pending_state() {
    let (state, root) =
        test_state_with_session_manager("session-resolve-question", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let session_path = root.join("sessions").join(format!("{}.jsonl", session.id));
    let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("session tape should load");
    tape.record_question_requested(&sample_question_request());
    tape.save_jsonl(&session_path).expect("session tape should save");

    let response = handlers::resolve_pending_question(
        State(state.clone()),
        Json(ResolvePendingQuestionRequest {
            session_id: Some(session.id.clone()),
            result: QuestionResult {
                status: QuestionResultStatus::Answered,
                request_id: "qreq_123".into(),
                answers: vec![QuestionAnswer {
                    question_id: "database".into(),
                    selected_option_ids: vec!["sqlite".into()],
                    text: None,
                }],
                reason: None,
            },
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response).await;
    assert_eq!(body.get("ok"), Some(&serde_json::json!(true)));

    let restored = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("updated tape should load");
    assert_eq!(
        restored.try_pending_question_request().expect("pending question should decode"),
        None
    );
    assert!(restored.entries().iter().any(|entry| entry.event_name() == Some("question_resolved")));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_pending_question_without_waiter_only_records_resolution() {
    let (state, root) =
        test_state_with_session_manager("session-resolve-no-waiter", ProviderRegistry::default());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let session_path = root.join("sessions").join(format!("{}.jsonl", session.id));
    let request = sample_question_request();
    let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("session tape should load");
    tape.record_question_requested(&request);
    tape.save_jsonl(&session_path).expect("session tape should save");

    let response = handlers::resolve_pending_question(
        State(state.clone()),
        Json(ResolvePendingQuestionRequest {
            session_id: Some(session.id.clone()),
            result: QuestionResult {
                status: QuestionResultStatus::Answered,
                request_id: request.request_id.clone(),
                answers: vec![QuestionAnswer {
                    question_id: "database".into(),
                    selected_option_ids: vec!["sqlite".into()],
                    text: None,
                }],
                reason: None,
            },
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);

    let restored = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("updated tape should load");
    assert_eq!(
        restored.try_pending_question_request().expect("pending question should decode"),
        None
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn resolve_pending_question_does_not_block_while_ask_question_is_waiting() {
    let (state, root) =
        test_state_with_session_manager("session-resolve-while-waiting", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let request = sample_question_request();
    let request_id = request.request_id.clone();
    let session_id = session.id.clone();
    let session_path = root.join("sessions").join(format!("{}.jsonl", session_id));
    let ask_handle = tokio::spawn({
        let session_manager = state.session_manager.clone();
        let session_id = session_id.clone();
        let request = request.clone();
        async move { session_manager.ask_question(session_id, request).await }
    });

    let pending_ready = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        loop {
            let tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
                .expect("session tape should load");
            let pending =
                tape.try_pending_question_request().expect("pending question should decode");
            if pending.as_ref().map(|value| value.request_id.as_str()) == Some(request_id.as_str())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(pending_ready.is_ok(), "pending question should be recorded before resolution");

    let resolution = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        state.session_manager.resolve_pending_question(
            session_id.clone(),
            QuestionResult {
                status: QuestionResultStatus::Answered,
                request_id: request_id.clone(),
                answers: vec![QuestionAnswer {
                    question_id: "database".into(),
                    selected_option_ids: vec!["sqlite".into()],
                    text: None,
                }],
                reason: None,
            },
        ),
    )
    .await;

    assert!(
        resolution.is_ok(),
        "resolving a pending question should not stall behind ask_question"
    );
    resolution
        .expect("resolution request should finish before timeout")
        .expect("pending question should resolve successfully");

    let ask_result = tokio::time::timeout(std::time::Duration::from_millis(500), ask_handle)
        .await
        .expect("ask_question task should finish after resolution")
        .expect("ask_question task should join successfully")
        .expect("ask_question should return the resolved result");
    assert_eq!(ask_result.status, QuestionResultStatus::Answered);
    assert_eq!(ask_result.request_id, request_id);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn cancel_turn_unblocks_ask_question_waiter() {
    let (state, root) =
        test_state_with_session_manager("session-cancel-while-waiting", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let request = sample_question_request();
    let session_id = session.id.clone();
    let session_path = root.join("sessions").join(format!("{}.jsonl", session_id));
    let ask_handle = tokio::spawn({
        let session_manager = state.session_manager.clone();
        let session_id = session_id.clone();
        let request = request.clone();
        async move { session_manager.ask_question(session_id, request).await }
    });

    tokio::time::timeout(std::time::Duration::from_millis(500), async {
        loop {
            let tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
                .expect("session tape should load");
            if tape
                .try_pending_question_request()
                .expect("pending question should decode")
                .is_some()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("pending question should be recorded before cancel turn");

    let cancelled = state
        .session_manager
        .cancel_turn(session_id.clone())
        .await
        .expect("cancel turn request should succeed");
    assert!(cancelled, "cancel turn should report a running turn");

    let ask_result = tokio::time::timeout(std::time::Duration::from_millis(500), ask_handle)
        .await
        .expect("ask_question task should finish after cancel turn")
        .expect("ask_question task should join successfully")
        .expect("ask_question should return a cancellation result");
    assert_eq!(ask_result.status, QuestionResultStatus::Cancelled);
    assert_eq!(ask_result.request_id, request.request_id);

    let restored = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("updated tape should load");
    assert_eq!(
        restored.try_pending_question_request().expect("pending question should decode"),
        None
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn ask_question_rejects_second_pending_question_in_same_session() {
    let (state, root) =
        test_state_with_session_manager("session-duplicate-pending-question", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let first_request = sample_question_request();
    let session_id = session.id.clone();
    let session_path = root.join("sessions").join(format!("{}.jsonl", session_id));
    let first_handle = tokio::spawn({
        let session_manager = state.session_manager.clone();
        let session_id = session_id.clone();
        let request = first_request.clone();
        async move { session_manager.ask_question(session_id, request).await }
    });

    tokio::time::timeout(std::time::Duration::from_millis(500), async {
        loop {
            let tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
                .expect("session tape should load");
            if tape
                .try_pending_question_request()
                .expect("pending question should decode")
                .is_some()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first pending question should be recorded");

    let second_result = state
        .session_manager
        .ask_question(
            session_id.clone(),
            QuestionRequest {
                request_id: "qreq_456".into(),
                invocation_id: "call_456".into(),
                turn_id: "turn_456".into(),
                questions: vec![QuestionItem {
                    id: "runtime".into(),
                    question: "Use which runtime?".into(),
                    kind: QuestionKind::Choice,
                    required: true,
                    multi_select: false,
                    options: Vec::new(),
                    placeholder: None,
                    recommended_option_id: None,
                    recommendation_reason: None,
                }],
            },
        )
        .await;

    let error = second_result.expect_err("second pending question should be rejected");
    assert!(
        error.message.contains("session already has a pending question"),
        "unexpected error: {}",
        error.message
    );

    state
        .session_manager
        .cancel_pending_question(session_id)
        .await
        .expect("cleanup cancel pending question should succeed");
    first_handle
        .await
        .expect("first ask_question should join successfully")
        .expect("first ask_question should resolve after cleanup");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn cancel_pending_question_records_cancelled_result() {
    let (state, root) =
        test_state_with_session_manager("session-cancel-question", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let session_path = root.join("sessions").join(format!("{}.jsonl", session.id));
    let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("session tape should load");
    tape.record_question_requested(&sample_question_request());
    tape.save_jsonl(&session_path).expect("session tape should save");

    let response = handlers::cancel_pending_question(
        State(state.clone()),
        axum::extract::Query(SessionQuery {
            session_id: Some(session.id),
            before_turn_id: None,
            limit: None,
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let restored = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("updated tape should load");
    let cancelled = restored
        .entries()
        .iter()
        .find(|entry| entry.event_name() == Some("question_resolved"))
        .and_then(|entry| entry.event_data())
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str());
    assert_eq!(cancelled, Some("cancelled"));

    let _ = std::fs::remove_dir_all(root);
}
