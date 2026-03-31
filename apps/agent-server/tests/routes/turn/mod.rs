use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use provider_registry::{ProviderAccount, ProviderRegistry};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use super::{CancelTurnRequest, TurnRequest, handlers, handlers::map_broadcast_result};
use crate::routes::test_support::{
    test_state_with_session_manager, test_state_with_session_manager_setup,
};

fn sample_registry() -> ProviderRegistry {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount::openai_responses(
        "primary",
        "https://primary.example.com",
        "primary-key",
        "model-primary",
    ));
    registry
}

async fn response_body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
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
fn lagged_broadcast_result_maps_to_sync_required_event() {
    let mapped = map_broadcast_result(Err(BroadcastStreamRecvError::Lagged(5)));
    assert!(mapped.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn submit_turn_rejects_session_waiting_for_question_response() {
    let (state, root) = test_state_with_session_manager("turn-pending-question", sample_registry());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let session_path = root.join("sessions").join(format!("{}.jsonl", session.id));
    let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("session tape should load");
    tape.record_question_requested(&agent_core::QuestionRequest {
        request_id: "qreq_123".into(),
        invocation_id: "call_123".into(),
        turn_id: "turn_123".into(),
        questions: vec![agent_core::QuestionItem {
            id: "database".into(),
            question: "Use which database?".into(),
            kind: agent_core::QuestionKind::Choice,
            required: true,
            multi_select: false,
            options: Vec::new(),
            placeholder: None,
            recommended_option_id: None,
            recommendation_reason: None,
        }],
    });
    tape.save_jsonl(&session_path).expect("session tape should save");

    let response = handlers::submit_turn(
        State(state.clone()),
        Json(TurnRequest { prompt: "keep going".into(), session_id: Some(session.id) }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_body_json(response).await;
    assert_eq!(
        body.get("error"),
        Some(&serde_json::json!("session is waiting for a question response"))
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn submit_turn_allows_new_message_after_restart_clears_stale_incomplete_turn() {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let root = std::env::temp_dir().join(format!("aia-routes-stale-turn-{suffix}"));
    let session_id = "session-stale-turn";

    let (state, root) =
        test_state_with_session_manager_setup(root, sample_registry(), |root, store| {
            store
                .create_session(&agent_store::SessionRecord::new(
                    session_id.to_string(),
                    "Stale Turn".to_string(),
                    "model-primary".to_string(),
                ))
                .expect("session record should be inserted directly into store");

            let session_path = root.join("sessions").join(format!("{session_id}.jsonl"));
            let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
                .expect("session tape should load");
            tape.append_entry(
                session_tape::TapeEntry::message(&agent_core::Message::new(
                    agent_core::Role::User,
                    "处理中",
                ))
                .with_run_id("turn-stale"),
            );
            tape.append_entry(
                session_tape::TapeEntry::thinking("先分析").with_run_id("turn-stale"),
            );
            tape.save_jsonl(&session_path).expect("session tape should save");
        });

    let response = handlers::submit_turn(
        State(state.clone()),
        Json(TurnRequest { prompt: "keep going".into(), session_id: Some(session_id.into()) }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = response_body_json(response).await;
    assert_eq!(body.get("ok"), Some(&serde_json::json!(true)));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn submit_turn_ignores_auto_compress_failure_before_turn_start() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept should succeed");
            let mut buffer = [0_u8; 8192];
            let _ = stream.read(&mut buffer).expect("request should be readable");
            let response = concat!(
                "HTTP/1.1 502 Bad Gateway\r\n",
                "content-type: application/json\r\n",
                "connection: close\r\n",
                "content-length: 2\r\n\r\n",
                "{}"
            );
            stream.write_all(response.as_bytes()).expect("response write should succeed");
        }
    });

    let mut registry = ProviderRegistry::default();
    let mut provider = ProviderAccount::openai_responses(
        "primary",
        format!("http://{address}"),
        "test-key",
        "model-primary",
    );
    provider.models[0].limit =
        Some(provider_registry::ModelLimit { context: Some(100), output: Some(64) });
    registry.upsert(provider);

    let (state, root) = test_state_with_session_manager("turn-auto-compress-502", registry.clone());
    let session = state
        .session_manager
        .create_session(Some("Session One".into()))
        .await
        .expect("session should be created");

    let session_path = root.join("sessions").join(format!("{}.jsonl", session.id));
    let mut tape = session_tape::SessionTape::load_jsonl_or_default(&session_path)
        .expect("session tape should load");
    for content in ["u1", "a1", "u2", "a2"] {
        tape.append_entry(session_tape::TapeEntry::message(&agent_core::Message::new(
            if content.starts_with('u') {
                agent_core::Role::User
            } else {
                agent_core::Role::Assistant
            },
            content,
        )));
    }
    tape.append_entry(session_tape::TapeEntry::event(
        "turn_completed",
        Some(serde_json::json!({
            "status": "ok",
            "usage": {
                "input_tokens": 95,
                "output_tokens": 10,
                "total_tokens": 105,
                "cached_tokens": 0
            }
        })),
    ));
    tape.save_jsonl(&session_path).expect("session tape should save");

    let response = handlers::submit_turn(
        State(state.clone()),
        Json(TurnRequest { prompt: "keep going".into(), session_id: Some(session.id.clone()) }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = response_body_json(response).await;
    assert_eq!(body.get("ok"), Some(&serde_json::json!(true)));

    handle.join().expect("server thread should exit");
    let _ = std::fs::remove_dir_all(root);
}
