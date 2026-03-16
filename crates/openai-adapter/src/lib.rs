mod chat_completions;
mod error;
mod mapping;
mod payloads;
mod responses;
mod streaming;

#[cfg(test)]
mod tests;

pub use chat_completions::{OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel};
pub use error::OpenAiAdapterError;
pub(crate) use mapping::{
    chat_completion_messages, extract_reasoning_stream_text, extract_stream_text,
    parse_tool_arguments, responses_input_item,
};
pub(crate) use payloads::{
    ChatCompletionsResponse, ChatCompletionsUsage, ReasoningSummaryPart, ResponsesContent,
    ResponsesOutput, ResponsesResponse, ResponsesUsage,
};
pub(crate) use streaming::stream_lines_with_abort;
pub use responses::{OpenAiResponsesConfig, OpenAiResponsesModel};
