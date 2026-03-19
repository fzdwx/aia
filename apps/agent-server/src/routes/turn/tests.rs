use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use super::{dto::CancelTurnRequest, handlers::map_broadcast_result};

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
