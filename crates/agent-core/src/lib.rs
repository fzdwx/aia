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
    Completion, CompletionRequest, CompletionSegment, ModelCheckpoint, ModelDisposition,
    ModelIdentity,
};
pub use conversation::{ConversationItem, Message, Role};
pub use error::CoreError;
pub use registry::ToolRegistry;
pub use streaming::{
    AbortSignal, StreamEvent, ToolExecutionContext, ToolOutputDelta, ToolOutputStream,
};
pub use tooling::{ToolCall, ToolDefinition, ToolResult};
pub use traits::{LanguageModel, Tool, ToolExecutor};
