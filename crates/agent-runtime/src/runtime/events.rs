use agent_core::{LanguageModel, ToolExecutor};

use crate::{RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};

use super::{AgentRuntime, RuntimeError};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub fn subscribe(&mut self) -> RuntimeSubscriberId {
        let subscriber_id = self.next_subscriber_id;
        self.next_subscriber_id += 1;
        self.subscribers.insert(subscriber_id, self.events.len());
        subscriber_id
    }

    pub fn collect_events(
        &mut self,
        subscriber_id: RuntimeSubscriberId,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let cursor = self
            .subscribers
            .get_mut(&subscriber_id)
            .ok_or_else(|| RuntimeError::subscription(format!("订阅者不存在：{subscriber_id}")))?;
        let events = self.events[*cursor..].to_vec();
        *cursor = self.events.len();
        Ok(events)
    }

    pub(super) fn publish_event(&mut self, event: RuntimeEvent) {
        self.events.push(event);
    }

    pub(super) fn publish_turn_lifecycle(&mut self, turn: TurnLifecycle) {
        self.publish_event(RuntimeEvent::TurnLifecycle { turn });
    }
}
