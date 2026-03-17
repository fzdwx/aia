use async_trait::async_trait;

use crate::{
    AbortSignal, Completion, CompletionRequest, CoreError, StreamEvent, ToolDefinition,
    ToolExecutionContext, ToolOutputDelta, ToolResult,
};

#[async_trait(?Send)]
pub trait LanguageModel {
    type Error: std::error::Error;

    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error>;

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        let completion = self.complete(request).await?;
        sink(StreamEvent::Done);
        Ok(completion)
    }

    async fn complete_streaming_with_abort(
        &self,
        request: CompletionRequest,
        abort: &AbortSignal,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        let _ = abort;
        self.complete_streaming(request, sink).await
    }

    fn is_cancelled_error(_error: &Self::Error) -> bool {
        false
    }
}

#[async_trait(?Send)]
pub trait ToolExecutor {
    type Error: std::error::Error;

    fn definitions(&self) -> Vec<ToolDefinition>;

    async fn call(
        &self,
        call: &crate::ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error>;
}

#[async_trait(?Send)]
pub trait Tool: Send {
    fn name(&self) -> &str;

    fn definition(&self) -> ToolDefinition;

    async fn call(
        &self,
        tool_call: &crate::ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError>;
}
