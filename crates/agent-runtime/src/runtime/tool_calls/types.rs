use std::collections::BTreeMap;

use agent_core::{AbortSignal, LlmTraceRequestContext, StreamEvent, ToolCall};

use crate::{ToolInvocationOutcome, ToolTraceContext};

use super::super::helpers::PreviousToolCall;

pub(in super::super) struct ExecuteToolCallContext<'a> {
    pub(super) turn_id: &'a str,
    pub(super) parent_trace_context: Option<&'a LlmTraceRequestContext>,
    pub(super) assistant_entry_id: Option<u64>,
    pub(super) tool_call_entry_id: u64,
    pub(super) call: &'a ToolCall,
    pub(super) seen_tool_calls: &'a mut BTreeMap<String, PreviousToolCall>,
    pub(super) source_entry_ids: &'a mut Vec<u64>,
    pub(super) abort_signal: AbortSignal,
}

pub(super) struct ToolCallLifecycleContext<'a> {
    pub(super) turn_id: &'a str,
    pub(super) assistant_entry_id: Option<u64>,
    pub(super) tool_call_entry_id: u64,
    pub(super) call: &'a ToolCall,
    pub(super) started_at_ms: u64,
    pub(super) tool_trace_context: Option<ToolTraceContext>,
    pub(super) source_entry_ids: &'a mut Vec<u64>,
}

pub(super) struct FailedToolCallContext<'a> {
    pub(super) lifecycle: ToolCallLifecycleContext<'a>,
    pub(super) event_name: &'a str,
}

impl ExecuteToolCallContext<'_> {
    pub(in super::super) fn new<'a>(
        turn_id: &'a str,
        parent_trace_context: Option<&'a LlmTraceRequestContext>,
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: &'a ToolCall,
        seen_tool_calls: &'a mut BTreeMap<String, PreviousToolCall>,
        source_entry_ids: &'a mut Vec<u64>,
        abort_signal: AbortSignal,
    ) -> ExecuteToolCallContext<'a> {
        ExecuteToolCallContext {
            turn_id,
            parent_trace_context,
            assistant_entry_id,
            tool_call_entry_id,
            call,
            seen_tool_calls,
            source_entry_ids,
            abort_signal,
        }
    }

    pub(super) fn lifecycle_context(
        &mut self,
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
    ) -> ToolCallLifecycleContext<'_> {
        ToolCallLifecycleContext {
            turn_id: self.turn_id,
            assistant_entry_id: self.assistant_entry_id,
            tool_call_entry_id: self.tool_call_entry_id,
            call: self.call,
            started_at_ms,
            tool_trace_context,
            source_entry_ids: self.source_entry_ids,
        }
    }

    pub(super) fn started_event(&self) -> StreamEvent {
        StreamEvent::ToolCallStarted {
            invocation_id: self.call.invocation_id.clone(),
            tool_name: self.call.tool_name.clone(),
            arguments: self.call.arguments.clone(),
        }
    }

    pub(super) fn remember_outcome(
        &mut self,
        call_signature: &str,
        outcome: &ToolInvocationOutcome,
    ) {
        self.seen_tool_calls
            .insert(call_signature.to_string(), PreviousToolCall::from_outcome(outcome));
    }
}
