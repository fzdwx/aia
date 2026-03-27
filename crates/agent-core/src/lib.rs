extern crate self as agent_core;

mod completion;
mod conversation;
mod error;
mod interaction;
mod question;
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
    PromptCacheRetention, ReasoningEffort, RequestTimeoutConfig,
};
pub use conversation::{ConversationItem, Message, Role};
pub use error::CoreError;
pub use interaction::SessionInteractionCapabilities;
pub use question::{
    QuestionAnswer, QuestionItem, QuestionKind, QuestionOption, QuestionRequest, QuestionResult,
    QuestionResultStatus,
};
pub use registry::ToolRegistry;
pub use streaming::{
    AbortSignal, RuntimeToolContext, RuntimeToolContextStats, StreamEvent, ToolExecutionContext,
    ToolOutputDelta, ToolOutputStream,
};
pub use tooling::{
    PendingToolRequest, ToolArgsSchema, ToolCall, ToolCallOutcome, ToolDefinition, ToolResult,
    ToolSchema, ToolSchemaMetadataValue, ToolSchemaProperty,
};
pub use traits::{LanguageModel, Tool, ToolExecutor};
