use std::collections::BTreeSet;
use std::sync::Arc;

use agent_core::{
    LanguageModel, RuntimeToolContext, StreamEvent, ToolCallOutcome, ToolExecutionContext,
    ToolExecutor, ToolOutputDelta, ToolResult,
};

use crate::{ToolInvocationLifecycle, ToolTraceContext};

use super::super::{
    AgentRuntime, RuntimeError,
    helpers::{build_tool_trace_context, now_timestamp_ms},
    runtime_context_bridge,
};
use super::types::{
    ExecuteToolCallContext, FailedToolCallContext, PreparedToolCallOutcome, ToolCallExecutionResult,
};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(in super::super) async fn execute_tool_call(
        &mut self,
        context: ExecuteToolCallContext<'_>,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolCallExecutionResult, RuntimeError> {
        let mut context = context;
        let started_at_ms = now_timestamp_ms();
        on_delta(context.started_event(started_at_ms));
        let prepared = self.prepare_tool_call(&context, started_at_ms, on_delta).await;
        self.commit_prepared_tool_call(&mut context, prepared, on_delta)
    }

    pub(in super::super) async fn prepare_tool_call(
        &mut self,
        context: &ExecuteToolCallContext<'_>,
        started_at_ms: u64,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> PreparedToolCallOutcome {
        let runtime_bridge = runtime_context_bridge::RuntimeToolContextBridge::new(self);
        let runtime_context: Arc<dyn RuntimeToolContext> = runtime_bridge.clone();
        let tool_trace_context =
            context.parent_trace_context.map(|trace| build_tool_trace_context(trace, context.call));
        let available_tool_names = self
            .visible_tools()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();
        let tool_name = context.call.tool_name.clone();

        if context.abort_signal.is_aborted() {
            return PreparedToolCallOutcome::Failed {
                started_at_ms,
                tool_trace_context,
                event_name: "tool_call_cancelled",
                runtime_error: RuntimeError::cancelled(),
            };
        }

        if !available_tool_names.contains(&tool_name) {
            return PreparedToolCallOutcome::Failed {
                started_at_ms,
                tool_trace_context,
                event_name: "tool_call_rejected",
                runtime_error: RuntimeError::tool_unavailable(tool_name),
            };
        }

        match self
            .tools
            .call(
                context.call,
                &mut |delta: ToolOutputDelta| {
                    on_delta(StreamEvent::ToolOutputDelta {
                        invocation_id: context.call.invocation_id.clone(),
                        stream: delta.stream,
                        text: delta.text,
                    });
                },
                &ToolExecutionContext {
                    run_id: context.turn_id.to_string(),
                    session_id: self.session_id.clone(),
                    workspace_root: self.workspace_root.clone(),
                    abort: context.abort_signal.clone(),
                    runtime: Some(runtime_context),
                },
            )
            .await
        {
            Ok(outcome) => {
                if let Err(runtime_error) =
                    self.apply_runtime_tool_handoffs(context.turn_id, &runtime_bridge)
                {
                    return PreparedToolCallOutcome::Failed {
                        started_at_ms,
                        tool_trace_context,
                        event_name: "tool_call_failed",
                        runtime_error,
                    };
                }
                match outcome {
                    ToolCallOutcome::Completed { result } => {
                        if result.invocation_id != context.call.invocation_id
                            || result.tool_name != context.call.tool_name
                        {
                            PreparedToolCallOutcome::Failed {
                                started_at_ms,
                                tool_trace_context,
                                event_name: "tool_result_rejected",
                                runtime_error: RuntimeError::tool_result_mismatch(
                                    context.call,
                                    &result,
                                ),
                            }
                        } else {
                            PreparedToolCallOutcome::Completed {
                                started_at_ms,
                                tool_trace_context,
                                result,
                                failed: context.abort_signal.is_aborted(),
                            }
                        }
                    }
                    ToolCallOutcome::Suspended { request } => {
                        if request.invocation_id != context.call.invocation_id
                            || request.tool_name != context.call.tool_name
                            || request.turn_id != context.turn_id
                        {
                            PreparedToolCallOutcome::Failed {
                                started_at_ms,
                                tool_trace_context,
                                event_name: "tool_request_rejected",
                                runtime_error: RuntimeError::tool(format!(
                                    "pending tool request mismatch: call {}#{}, request {}#{}",
                                    context.call.tool_name,
                                    context.call.invocation_id,
                                    request.tool_name,
                                    request.invocation_id,
                                )),
                            }
                        } else {
                            PreparedToolCallOutcome::Suspended { started_at_ms, request }
                        }
                    }
                }
            }
            Err(error) => PreparedToolCallOutcome::Failed {
                started_at_ms,
                tool_trace_context,
                event_name: "tool_call_failed",
                runtime_error: RuntimeError::tool(error),
            },
        }
    }

    pub(in super::super) fn commit_prepared_tool_call(
        &mut self,
        context: &mut ExecuteToolCallContext<'_>,
        prepared: PreparedToolCallOutcome,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolCallExecutionResult, RuntimeError> {
        match prepared {
            PreparedToolCallOutcome::Completed {
                started_at_ms,
                tool_trace_context,
                result,
                failed,
            } => self
                .record_completed_tool_call_for_context(
                    context,
                    started_at_ms,
                    tool_trace_context,
                    result,
                    failed,
                    on_delta,
                )
                .map(ToolCallExecutionResult::Completed),
            PreparedToolCallOutcome::Suspended { started_at_ms, request } => self
                .record_suspended_tool_call_for_context(context, started_at_ms, request)
                .map(ToolCallExecutionResult::Suspended),
            PreparedToolCallOutcome::Failed {
                started_at_ms,
                tool_trace_context,
                event_name,
                runtime_error,
            } => self
                .record_failed_tool_call_for_context(
                    context,
                    started_at_ms,
                    tool_trace_context,
                    event_name,
                    runtime_error,
                    on_delta,
                )
                .map(ToolCallExecutionResult::Completed),
        }
    }

    fn record_completed_tool_call_for_context(
        &mut self,
        context: &mut ExecuteToolCallContext<'_>,
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
        result: ToolResult,
        failed: bool,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        self.record_completed_tool_call(
            context.lifecycle_context(started_at_ms, tool_trace_context),
            result,
            failed,
            on_delta,
        )
    }

    fn record_failed_tool_call_for_context(
        &mut self,
        context: &mut ExecuteToolCallContext<'_>,
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
        event_name: &'static str,
        runtime_error: RuntimeError,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        self.record_failed_tool_call(
            FailedToolCallContext {
                lifecycle: context.lifecycle_context(started_at_ms, tool_trace_context),
                event_name,
            },
            runtime_error,
            on_delta,
        )
    }

    fn record_suspended_tool_call_for_context(
        &mut self,
        context: &mut ExecuteToolCallContext<'_>,
        started_at_ms: u64,
        request: agent_core::PendingToolRequest,
    ) -> Result<agent_core::PendingToolRequest, RuntimeError> {
        self.record_suspended_tool_call(context.lifecycle_context(started_at_ms, None), request)
    }
}
