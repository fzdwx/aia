mod chat_completions;
mod error;
mod mapping;
mod payloads;
mod responses;

#[cfg(test)]
mod tests;

pub use chat_completions::{OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel};
pub use error::OpenAiAdapterError;
pub(crate) use mapping::{
    chat_completion_messages, extract_reasoning_stream_text, extract_stream_text,
    parse_tool_arguments, responses_continuation, responses_input_item,
};
pub(crate) use payloads::{
    ChatCompletionsResponse, ChatCompletionsUsage, ReasoningSummaryPart, ResponsesContent,
    ResponsesOutput, ResponsesResponse, ResponsesUsage,
};
pub use responses::{OpenAiResponsesConfig, OpenAiResponsesModel};
