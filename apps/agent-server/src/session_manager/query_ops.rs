use std::collections::HashMap;

use crate::{runtime_worker::CurrentTurnSnapshot, sse::TurnStatus};
use agent_core::{QuestionResult, QuestionResultStatus};
use agent_runtime::{ContextStats, TurnLifecycle};

use super::{
    RuntimeWorkerError, SessionId, SessionSlot, SlotStatus, load_session_tape_with_repair,
    read_lock, update_current_turn_status,
};

pub(crate) struct SessionQueryService<'a> {
    slots: &'a mut HashMap<SessionId, SessionSlot>,
    hydration_errors: &'a HashMap<SessionId, RuntimeWorkerError>,
}

impl<'a> SessionQueryService<'a> {
    pub(crate) fn new(
        slots: &'a mut HashMap<SessionId, SessionSlot>,
        hydration_errors: &'a HashMap<SessionId, RuntimeWorkerError>,
    ) -> Self {
        Self { slots, hydration_errors }
    }

    fn get_slot(&self, session_id: &str) -> Result<&SessionSlot, RuntimeWorkerError> {
        self.slots.get(session_id).ok_or_else(|| {
            self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            })
        })
    }

    fn get_slot_mut(&mut self, session_id: &str) -> Result<&mut SessionSlot, RuntimeWorkerError> {
        if self.slots.contains_key(session_id) {
            return self.slots.get_mut(session_id).ok_or_else(|| {
                RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
            });
        }

        Err(self.hydration_errors.get(session_id).cloned().unwrap_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        }))
    }

    pub(crate) fn cancel_turn(&mut self, session_id: &str) -> Result<bool, RuntimeWorkerError> {
        let slot = self.get_slot_mut(session_id)?;

        let pending_request = load_session_tape_with_repair(&slot.session_path)?
            .try_pending_question_request()
            .map_err(|error| RuntimeWorkerError::internal(error.to_string()))?;

        if slot.status() != SlotStatus::Running {
            if let Some(request) = pending_request {
                if let Some(waiter) = slot.remove_pending_question_waiter(&request.request_id) {
                    let _ = waiter.send(QuestionResult {
                        status: QuestionResultStatus::Cancelled,
                        request_id: request.request_id,
                        answers: Vec::new(),
                        reason: None,
                    });
                    update_current_turn_status(&slot.current_turn, TurnStatus::Cancelled);
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        let Some(running_turn) = slot.running_turn() else {
            return Err(RuntimeWorkerError::internal("running turn handle missing"));
        };

        running_turn.control.cancel();
        update_current_turn_status(&slot.current_turn, TurnStatus::Cancelled);
        if let Some(request) = pending_request {
            if let Some(waiter) = slot.remove_pending_question_waiter(&request.request_id) {
                let _ = waiter.send(QuestionResult {
                    status: QuestionResultStatus::Cancelled,
                    request_id: request.request_id,
                    answers: Vec::new(),
                    reason: None,
                });
            }
        }
        Ok(true)
    }

    pub(crate) fn history(
        &self,
        session_id: &str,
    ) -> Result<Vec<TurnLifecycle>, RuntimeWorkerError> {
        let slot = self.get_slot(session_id)?;
        Ok(read_lock(&slot.history).clone())
    }

    pub(crate) fn current_turn(
        &self,
        session_id: &str,
    ) -> Result<Option<CurrentTurnSnapshot>, RuntimeWorkerError> {
        let slot = self.get_slot(session_id)?;
        Ok(read_lock(&slot.current_turn).clone())
    }

    pub(crate) fn session_info(
        &self,
        session_id: &str,
    ) -> Result<ContextStats, RuntimeWorkerError> {
        let slot = self.get_slot(session_id)?;

        if let Some(runtime) = slot.runtime() {
            return Ok(runtime.context_stats());
        }

        Ok(read_lock(&slot.context_stats).clone())
    }
}
