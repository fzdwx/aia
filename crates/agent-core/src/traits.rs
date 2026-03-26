use async_trait::async_trait;

use crate::{
    AbortSignal, Completion, CompletionRequest, CoreError, SessionInteractionCapabilities,
    StreamEvent, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};

#[async_trait]
pub trait LanguageModel: Send + Sync {
    type Error: std::error::Error;

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        abort: &AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error>;

    fn is_cancelled_error(_error: &Self::Error) -> bool {
        false
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    type Error: std::error::Error;

    fn definitions(&self) -> Vec<ToolDefinition>;

    fn definitions_for_capabilities(
        &self,
        _capabilities: &SessionInteractionCapabilities,
    ) -> Vec<ToolDefinition> {
        self.definitions()
    }

    fn tool_requires_runtime_context(&self, _name: &str) -> bool {
        false
    }

    async fn call(
        &self,
        call: &crate::ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn definition(&self) -> ToolDefinition;

    /// Declares whether this tool should only be exposed when the current
    /// session explicitly supports interactive capabilities.
    ///
    /// Return `true` for tools whose use depends on an interactive surface,
    /// such as pending-question UI, structured input, or other user-mediated
    /// runtime flows. Tools that are always safe to expose should keep the
    /// default `false`.
    fn requires_interactive_capability(&self) -> bool {
        false
    }

    /// Declares whether this tool depends on runtime-provided context rather
    /// than only on its explicit arguments.
    ///
    /// Return `true` for tools that need access to `ToolExecutionContext`
    /// runtime adapters or hosts, for example to read tape/context stats,
    /// create handoffs, or invoke host-backed capabilities such as
    /// `Question`. Runtime-sensitive tools may need more conservative
    /// scheduling than pure parameter-driven tools.
    fn requires_runtime_context(&self) -> bool {
        false
    }

    async fn call(
        &self,
        tool_call: &crate::ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError>;
}
