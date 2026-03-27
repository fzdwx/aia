use async_trait::async_trait;

use crate::{
    AbortSignal, Completion, CompletionRequest, CoreError, StreamEvent, ToolCallOutcome,
    ToolDefinition, ToolExecutionContext, ToolOutputDelta,
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

    async fn call(
        &self,
        call: &crate::ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolCallOutcome, Self::Error>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn definition(&self) -> ToolDefinition;

    fn is_interactive_tool(&self) -> bool {
        false
    }

    async fn call(
        &self,
        tool_call: &crate::ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolCallOutcome, CoreError>;
}
