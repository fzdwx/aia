use std::collections::HashMap;

use agent_runtime::{ContextStats, TurnLifecycle};

use crate::{runtime_worker::CurrentTurnSnapshot, sse::TurnStatus};

use super::{
    RuntimeWorkerError, SessionId, SessionManagerConfig, SessionSlot, SlotStatus, read_lock,
    update_current_turn_status,
};

pub(crate) fn handle_cancel_turn(
    slots: &mut HashMap<SessionId, SessionSlot>,
    _config: &SessionManagerConfig,
    session_id: &str,
) -> Result<bool, RuntimeWorkerError> {
    let slot = slots
        .get_mut(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;

    if slot.status != SlotStatus::Running {
        return Ok(false);
    }

    let Some(running_turn) = slot.running_turn.as_ref() else {
        return Err(RuntimeWorkerError::internal("running turn handle missing"));
    };

    running_turn.control.cancel();
    update_current_turn_status(&slot.current_turn, TurnStatus::Cancelled);
    Ok(true)
}

pub(crate) fn handle_get_history(
    slots: &HashMap<SessionId, SessionSlot>,
    session_id: &str,
) -> Result<Vec<TurnLifecycle>, RuntimeWorkerError> {
    let slot = slots
        .get(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    Ok(read_lock(&slot.history).clone())
}

pub(crate) fn handle_get_current_turn(
    slots: &HashMap<SessionId, SessionSlot>,
    session_id: &str,
) -> Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError> {
    let slot = slots
        .get(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;
    Ok(read_lock(&slot.current_turn).clone())
}

pub(crate) fn handle_get_session_info(
    slots: &HashMap<SessionId, SessionSlot>,
    session_id: &str,
) -> Result<ContextStats, RuntimeWorkerError> {
    let slot = slots
        .get(session_id)
        .ok_or_else(|| RuntimeWorkerError::not_found(format!("session not found: {session_id}")))?;

    if let Some(runtime) = slot.runtime.as_ref() {
        return Ok(runtime.context_stats());
    }

    Ok(read_lock(&slot.context_stats).clone())
}
