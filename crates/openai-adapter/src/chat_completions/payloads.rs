use serde::Deserialize;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ChatCompletionsResponse {
    #[serde(default)]
    pub usage: Option<ChatCompletionsUsage>,
    pub choices: Vec<ChatCompletionChoice>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ChatCompletionsUsage {
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub prompt_tokens_details: Option<ChatCompletionsPromptTokensDetails>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ChatCompletionsPromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: Option<u64>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ChatCompletionChoice {
    pub message: ChatCompletionMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ChatCompletionMessage {
    #[serde(default)]
    pub content: Option<String>,
    // Some providers use "reasoning", others use "reasoning_content"
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ChatCompletionToolCall>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ChatCompletionToolCall {
    pub id: Option<String>,
    pub function: ChatCompletionFunction,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ChatCompletionFunction {
    pub name: String,
    pub arguments: String,
}
