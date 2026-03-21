use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use provider_registry::{ProviderProfile, ProviderRegistry};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{UpdateSessionSettingsRequest, handlers};
use crate::routes::test_support::{
    test_state_with_session_manager, test_state_with_session_manager_setup,
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
