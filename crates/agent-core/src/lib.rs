extern crate self as agent_core;

mod completion;
mod conversation;
mod error;
mod registry;
mod streaming;
mod tooling;
mod traits;

#[cfg(test)]
#[path = "../tests/lib/mod.rs"]
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
    ToolArgsSchema, ToolCall, ToolDefinition, ToolResult, ToolSchema, ToolSchemaMetadataValue,
    ToolSchemaProperty,
};
pub use traits::{LanguageModel, Tool, ToolExecutor};
