use agent_core::{
    AbortSignal, LlmTraceRequestContext, PendingToolRequest, StreamEvent, ToolCall, ToolResult,
};

use crate::ToolTraceContext;

pub(in super::super) struct ExecuteToolCallContext<'a> {
    pub(super) turn_id: &'a str,
    pub(super) parent_trace_context: Option<&'a LlmTraceRequestContext>,
    pub(super) assistant_entry_id: Option<u64>,
    pub(super) tool_call_entry_id: u64,
    pub(super) call: &'a ToolCall,
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

pub(crate) enum PreparedToolCallOutcome {
    Completed {
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
        result: ToolResult,
        failed: bool,
    },
    Suspended {
        started_at_ms: u64,
        request: PendingToolRequest,
    },
    Failed {
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
        event_name: &'static str,
        runtime_error: super::super::RuntimeError,
    },
}

pub(crate) enum ToolCallExecutionResult {
    Completed(crate::ToolInvocationLifecycle),
    Suspended(PendingToolRequest),
}

impl ExecuteToolCallContext<'_> {
    pub(in super::super) fn new<'a>(
        turn_id: &'a str,
        parent_trace_context: Option<&'a LlmTraceRequestContext>,
        assistant_entry_id: Option<u64>,
        tool_call_entry_id: u64,
        call: &'a ToolCall,
        source_entry_ids: &'a mut Vec<u64>,
        abort_signal: AbortSignal,
    ) -> ExecuteToolCallContext<'a> {
        ExecuteToolCallContext {
            turn_id,
            parent_trace_context,
            assistant_entry_id,
            tool_call_entry_id,
            call,
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

    pub(in super::super) fn started_event(&self, started_at_ms: u64) -> StreamEvent {
        StreamEvent::ToolCallStarted {
            invocation_id: self.call.invocation_id.clone(),
            tool_name: self.call.tool_name.clone(),
            arguments: self.call.arguments.clone(),
            started_at_ms,
        }
    }
}
