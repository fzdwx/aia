use std::collections::HashMap;

use agent_runtime::{ContextStats, TurnLifecycle};

use crate::{runtime_worker::CurrentTurnSnapshot, sse::TurnStatus};

use super::{
    RuntimeWorkerError, SessionId, SessionSlot, SlotStatus, read_lock, update_current_turn_status,
};

pub(crate) struct SessionQueryService<'a> {
    slots: &'a mut HashMap<SessionId, SessionSlot>,
}

impl<'a> SessionQueryService<'a> {
    pub(crate) fn new(slots: &'a mut HashMap<SessionId, SessionSlot>) -> Self {
        Self { slots }
    }

    pub(crate) fn cancel_turn(&mut self, session_id: &str) -> Result<bool, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

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

    pub(crate) fn history(
        &self,
        session_id: &str,
    ) -> Result<Vec<TurnLifecycle>, RuntimeWorkerError> {
        let slot = self.slots.get(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;
        Ok(read_lock(&slot.history).clone())
    }

    pub(crate) fn current_turn(
        &self,
        session_id: &str,
    ) -> Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError> {
        let slot = self.slots.get(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;
        Ok(read_lock(&slot.current_turn).clone())
    }

    pub(crate) fn session_info(
        &self,
        session_id: &str,
    ) -> Result<ContextStats, RuntimeWorkerError> {
        let slot = self.slots.get(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        if let Some(runtime) = slot.runtime.as_ref() {
            return Ok(runtime.context_stats());
        }

        Ok(read_lock(&slot.context_stats).clone())
    }
}
