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
                    created_at: "2026-03-21T00:00:00Z".into(),
                    updated_at: "2026-03-21T00:00:00Z".into(),
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
    assert!(restored.entries().iter().any(|entry| {
        entry.as_tool_result().is_some_and(|tool_result| tool_result.tool_name == "Question")
    }));

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
    assert!(restored.entries().iter().any(|entry| {
        entry.as_tool_result().is_some_and(|tool_result| tool_result.tool_name == "Question")
    }));

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
