mod completion;
mod conversation;
mod error;
mod registry;
mod streaming;
mod tooling;
mod traits;

#[cfg(test)]
mod tests;

pub use completion::{
    Completion, CompletionRequest, CompletionSegment, CompletionStopReason, CompletionUsage,
    LlmTraceRequestContext, ModelDisposition, ModelIdentity, ModelLimit, PromptCacheConfig,
    PromptCacheRetention, RequestTimeoutConfig,
};
pub use conversation::{ConversationItem, Message, Role};
pub use error::CoreError;
pub use registry::ToolRegistry;
pub use streaming::{
    AbortSignal, RuntimeToolContext, RuntimeToolContextStats, StreamEvent, ToolExecutionContext,
    ToolOutputDelta, ToolOutputStream,
};
pub use tooling::{
    ToolArgsSchema, ToolCall, ToolDefinition, ToolResult, ToolSchema, ToolSchemaProperty,
};
pub use traits::{LanguageModel, Tool, ToolExecutor};
