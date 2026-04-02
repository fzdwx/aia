use std::time::Duration;

use agent_core::StreamEvent;

use crate::OpenAiAdapterError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetryPolicy {
    pub(crate) max_attempts: u32,
    pub(crate) base_delay: Duration,
    pub(crate) max_delay: Duration,
    pub(crate) jitter: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(300),
            max_delay: Duration::from_millis(2000),
            jitter: Duration::from_millis(150),
        }
    }
}

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct StreamingAttemptState {
    emitted_visible_event: bool,
}

impl StreamingAttemptState {
    pub(crate) fn record_event(&mut self, event: &StreamEvent) {
        if is_visible_stream_event(event) {
            self.emitted_visible_event = true;
        }
    }

    pub(crate) fn can_retry(&self) -> bool {
        !self.emitted_visible_event
    }
}

pub(crate) fn is_visible_stream_event(event: &StreamEvent) -> bool {
    matches!(
        event,
        StreamEvent::ThinkingDelta { .. }
            | StreamEvent::TextDelta { .. }
            | StreamEvent::ToolCallDetected { .. }
            | StreamEvent::ToolCallStarted { .. }
    )
}

pub(crate) fn should_retry(error: &OpenAiAdapterError) -> bool {
    if error.is_cancelled() {
        return false;
    }

    if error.is_retryable() {
        return true;
    }

    match error.status_code() {
        Some(408 | 429 | 500 | 502 | 503 | 504) => true,
        Some(400 | 401 | 403 | 404 | 422) => false,
        Some(_) => false,
        None => true,
    }
}

pub(crate) fn backoff_delay(policy: RetryPolicy, attempt: u32) -> Duration {
    let factor = 1_u32.checked_shl(attempt.saturating_sub(1)).unwrap_or(u32::MAX);
    let scaled_base = policy.base_delay.saturating_mul(factor);
    let capped = scaled_base.min(policy.max_delay);
    capped.saturating_add(policy.jitter.min(Duration::from_millis(50)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_text_thinking_and_tool_detection_are_visible_events() {
        assert!(is_visible_stream_event(&StreamEvent::TextDelta {
            text: "hi".into()
        }));
        assert!(is_visible_stream_event(&StreamEvent::ThinkingDelta {
            text: "hmm".into()
        }));
        assert!(is_visible_stream_event(&StreamEvent::ToolCallDetected {
            invocation_id: "tool-1".into(),
            tool_name: "Read".into(),
            arguments: serde_json::json!({}),
            detected_at_ms: 1,
        }));
        assert!(!is_visible_stream_event(&StreamEvent::Log {
            text: "debug".into()
        }));
        assert!(!is_visible_stream_event(&StreamEvent::Done));
    }

    #[test]
    fn retryable_statuses_match_policy() {
        assert!(should_retry(&OpenAiAdapterError::new("err").with_status_code(Some(503))));
        assert!(should_retry(&OpenAiAdapterError::new("err").with_status_code(Some(429))));
        assert!(!should_retry(&OpenAiAdapterError::new("err").with_status_code(Some(400))));
        assert!(!should_retry(&OpenAiAdapterError::cancelled("cancelled")));
    }
}
