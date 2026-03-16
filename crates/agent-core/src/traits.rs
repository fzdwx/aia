use crate::{
    AbortSignal, Completion, CompletionRequest, CoreError, StreamEvent, ToolDefinition,
    ToolExecutionContext, ToolOutputDelta, ToolResult,
};

pub trait LanguageModel {
    type Error: std::error::Error;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error>;

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        let completion = self.complete(request)?;
        sink(StreamEvent::Done);
        Ok(completion)
    }

    fn complete_streaming_with_abort(
        &self,
        request: CompletionRequest,
        abort: &AbortSignal,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        let _ = abort;
        self.complete_streaming(request, sink)
    }

    fn is_cancelled_error(_error: &Self::Error) -> bool {
        false
    }
}

pub trait ToolExecutor {
    type Error: std::error::Error;

    fn definitions(&self) -> Vec<ToolDefinition>;

    fn call(
        &self,
        call: &crate::ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error>;
}

pub trait Tool: Send {
    fn name(&self) -> &str;

    fn definition(&self) -> ToolDefinition;

    fn call(
        &self,
        tool_call: &crate::ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError>;
}
