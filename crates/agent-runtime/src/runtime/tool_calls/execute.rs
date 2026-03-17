use std::collections::BTreeSet;
use std::sync::Arc;

use agent_core::{
    LanguageModel, RuntimeToolContext, StreamEvent, ToolExecutionContext, ToolExecutor,
    ToolOutputDelta, ToolResult,
};

use crate::{ToolInvocationLifecycle, ToolTraceContext};

use super::super::{
    AgentRuntime, RuntimeError,
    helpers::{build_tool_trace_context, now_timestamp_ms, tool_call_signature},
    tape_tools,
};
use super::types::{ExecuteToolCallContext, FailedToolCallContext, PreparedToolCallOutcome};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(in super::super) async fn execute_tool_call(
        &mut self,
        context: ExecuteToolCallContext<'_>,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let mut context = context;
        on_delta(context.started_event());
        let prepared = self.prepare_tool_call(&context, on_delta).await;
        self.commit_prepared_tool_call(&mut context, prepared, on_delta)
    }

    pub(in super::super) async fn prepare_tool_call(
        &mut self,
        context: &ExecuteToolCallContext<'_>,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> PreparedToolCallOutcome {
        let started_at_ms = now_timestamp_ms();
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

        if tape_tools::is_runtime_tool(&context.call.tool_name) {
            let prepared = match self.invoke_runtime_tool(context).await {
                Ok(result) => PreparedToolCallOutcome::Completed {
                    started_at_ms,
                    tool_trace_context,
                    result,
                    failed: context.abort_signal.is_aborted(),
                },
                Err(runtime_error) => PreparedToolCallOutcome::Failed {
                    started_at_ms,
                    tool_trace_context,
                    event_name: "tool_call_failed",
                    runtime_error,
                },
            };
            return prepared;
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
                    workspace_root: self.workspace_root.clone(),
                    abort: context.abort_signal.clone(),
                    runtime: None,
                },
            )
            .await
        {
            Ok(result) => {
                if result.invocation_id != context.call.invocation_id
                    || result.tool_name != context.call.tool_name
                {
                    PreparedToolCallOutcome::Failed {
                        started_at_ms,
                        tool_trace_context,
                        event_name: "tool_result_rejected",
                        runtime_error: RuntimeError::tool_result_mismatch(context.call, &result),
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
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let call_signature = tool_call_signature(context.call);
        match prepared {
            PreparedToolCallOutcome::Completed {
                started_at_ms,
                tool_trace_context,
                result,
                failed,
            } => self.record_completed_tool_call_for_context(
                context,
                started_at_ms,
                tool_trace_context,
                result,
                failed,
                &call_signature,
                on_delta,
            ),
            PreparedToolCallOutcome::Failed {
                started_at_ms,
                tool_trace_context,
                event_name,
                runtime_error,
            } => self.record_failed_tool_call_for_context(
                context,
                started_at_ms,
                tool_trace_context,
                event_name,
                runtime_error,
                &call_signature,
                on_delta,
            ),
        }
    }

    async fn invoke_runtime_tool(
        &mut self,
        context: &ExecuteToolCallContext<'_>,
    ) -> Result<ToolResult, RuntimeError> {
        let runtime_tools = tape_tools::build_runtime_tool_registry();
        let runtime_bridge = tape_tools::RuntimeToolContextBridge::new(self);
        let runtime_context: Arc<dyn RuntimeToolContext> = runtime_bridge.clone();
        let result = runtime_tools
            .call(
                context.call,
                &mut |_| {},
                &ToolExecutionContext {
                    run_id: context.turn_id.to_string(),
                    workspace_root: self.workspace_root.clone(),
                    abort: context.abort_signal.clone(),
                    runtime: Some(runtime_context),
                },
            )
            .await
            .map_err(RuntimeError::tool)?;
        self.apply_runtime_tool_handoffs(context.turn_id, &runtime_bridge)?;
        Ok(result)
    }

    fn record_completed_tool_call_for_context(
        &mut self,
        context: &mut ExecuteToolCallContext<'_>,
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
        result: ToolResult,
        failed: bool,
        call_signature: &str,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let lifecycle = self.record_completed_tool_call(
            context.lifecycle_context(started_at_ms, tool_trace_context),
            result,
            failed,
            on_delta,
        )?;
        context.remember_outcome(call_signature, &lifecycle.outcome);
        Ok(lifecycle)
    }

    fn record_failed_tool_call_for_context(
        &mut self,
        context: &mut ExecuteToolCallContext<'_>,
        started_at_ms: u64,
        tool_trace_context: Option<ToolTraceContext>,
        event_name: &'static str,
        runtime_error: RuntimeError,
        call_signature: &str,
        on_delta: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ToolInvocationLifecycle, RuntimeError> {
        let lifecycle = self.record_failed_tool_call(
            FailedToolCallContext {
                lifecycle: context.lifecycle_context(started_at_ms, tool_trace_context),
                event_name,
            },
            runtime_error,
            on_delta,
        )?;
        context.remember_outcome(call_signature, &lifecycle.outcome);
        Ok(lifecycle)
    }
}
