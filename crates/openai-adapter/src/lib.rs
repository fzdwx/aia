mod chat_completions;
mod error;
mod http;
mod mapping;
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
pub use responses::{OpenAiResponsesConfig, OpenAiResponsesModel};
pub(crate) use streaming::stream_lines_with_abort;
