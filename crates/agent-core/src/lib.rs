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
mod widget;

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
    AbortSignal, RuntimeToolContext, RuntimeToolContextStats, RuntimeToolHost, StreamEvent,
    ToolExecutionContext, ToolOutputDelta, ToolOutputStream,
};
pub use tooling::{
    ToolArgsSchema, ToolCall, ToolDefinition, ToolResult, ToolSchema, ToolSchemaMetadataValue,
    ToolSchemaProperty,
};
pub use traits::{LanguageModel, Tool, ToolExecutor};
pub use widget::{
    UiWidget, UiWidgetDocument, UiWidgetPhase, WidgetCanvasSnapshot, WidgetClientEvent,
    WidgetHostCommand,
};
