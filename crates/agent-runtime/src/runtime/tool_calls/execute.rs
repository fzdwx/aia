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
use super::types::{ExecuteToolCallContext, FailedToolCallContext};

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
        let started_at_ms = now_timestamp_ms();
        let tool_trace_context =
            context.parent_trace_context.map(|trace| build_tool_trace_context(trace, context.call));
        let available_tool_names = self
            .visible_tools()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();
        let call_signature = tool_call_signature(context.call);
        let tool_name = context.call.tool_name.clone();

        if context.abort_signal.is_aborted() {
            return self.record_failed_tool_call_for_context(
                &mut context,
                started_at_ms,
                tool_trace_context.clone(),
                "tool_call_cancelled",
                RuntimeError::cancelled(),
                &call_signature,
                on_delta,
            );
        }

        if !available_tool_names.contains(&tool_name) {
            return self.record_failed_tool_call_for_context(
                &mut context,
                started_at_ms,
                tool_trace_context.clone(),
                "tool_call_rejected",
                RuntimeError::tool_unavailable(tool_name),
                &call_signature,
                on_delta,
            );
        }

        if tape_tools::is_runtime_tool(&context.call.tool_name) {
            if context.abort_signal.is_aborted() {
                return self.record_failed_tool_call_for_context(
                    &mut context,
                    started_at_ms,
                    tool_trace_context.clone(),
                    "tool_call_cancelled",
                    RuntimeError::cancelled(),
                    &call_signature,
                    on_delta,
                );
            }

            on_delta(context.started_event());

            let result = self.invoke_runtime_tool(&context).await?;
            let was_cancelled = context.abort_signal.is_aborted();
            return self.record_completed_tool_call_for_context(
                &mut context,
                started_at_ms,
                tool_trace_context,
                result,
                was_cancelled,
                &call_signature,
                on_delta,
            );
        }

        on_delta(context.started_event());

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
                    let mismatch_error = RuntimeError::tool_result_mismatch(context.call, &result);
                    return self.record_failed_tool_call_for_context(
                        &mut context,
                        started_at_ms,
                        tool_trace_context.clone(),
                        "tool_result_rejected",
                        mismatch_error,
                        &call_signature,
                        on_delta,
                    );
                }

                let was_cancelled = context.abort_signal.is_aborted();
                self.record_completed_tool_call_for_context(
                    &mut context,
                    started_at_ms,
                    tool_trace_context,
                    result,
                    was_cancelled,
                    &call_signature,
                    on_delta,
                )
            }
            Err(error) => self.record_failed_tool_call_for_context(
                &mut context,
                started_at_ms,
                tool_trace_context,
                "tool_call_failed",
                RuntimeError::tool(error),
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
