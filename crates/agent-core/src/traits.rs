use crate::{
    Completion, CompletionRequest, CoreError, StreamEvent, ToolDefinition, ToolExecutionContext,
    ToolOutputDelta, ToolResult,
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
