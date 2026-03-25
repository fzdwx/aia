use channel_bridge::{ChannelCurrentTurnSnapshot, ChannelRuntimeEvent, ChannelTurnStatus};

use crate::{
    runtime_worker::CurrentTurnSnapshot,
    sse::{SsePayload, TurnStatus},
};

pub(super) fn map_sse_payload(payload: SsePayload) -> Option<ChannelRuntimeEvent> {
    match payload {
        SsePayload::CurrentTurnStarted { session_id, current_turn } => {
            Some(ChannelRuntimeEvent::CurrentTurnStarted {
                session_id,
                current_turn: map_current_turn_snapshot(current_turn),
            })
        }
        SsePayload::Status { session_id, turn_id, status } => Some(ChannelRuntimeEvent::Status {
            session_id,
            turn_id,
            status: map_turn_status(status),
        }),
        SsePayload::Stream { session_id, turn_id, event } => {
            Some(ChannelRuntimeEvent::Stream { session_id, turn_id, event })
        }
        SsePayload::TurnCompleted { session_id, turn_id, turn } => {
            Some(ChannelRuntimeEvent::TurnCompleted { session_id, turn_id, turn })
        }
        SsePayload::Error { session_id, turn_id, message } => {
            Some(ChannelRuntimeEvent::Error { session_id, turn_id, message })
        }
        _ => None,
    }
}

fn map_current_turn_snapshot(snapshot: CurrentTurnSnapshot) -> ChannelCurrentTurnSnapshot {
    ChannelCurrentTurnSnapshot {
        turn_id: snapshot.turn_id,
        started_at_ms: snapshot.started_at_ms,
        user_message: snapshot.user_message,
        status: map_turn_status(snapshot.status),
    }
}

fn map_turn_status(status: TurnStatus) -> ChannelTurnStatus {
    match status {
        TurnStatus::Waiting => ChannelTurnStatus::Waiting,
        TurnStatus::WaitingForQuestion => ChannelTurnStatus::Waiting,
        TurnStatus::Thinking => ChannelTurnStatus::Thinking,
        TurnStatus::Working => ChannelTurnStatus::Working,
        TurnStatus::Generating => ChannelTurnStatus::Generating,
        TurnStatus::Finishing => ChannelTurnStatus::Finishing,
        TurnStatus::Cancelled => ChannelTurnStatus::Cancelled,
    }
}
