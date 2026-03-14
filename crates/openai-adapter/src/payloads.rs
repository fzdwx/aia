use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct ResponsesResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub incomplete_details: Option<ResponsesIncompleteDetails>,
    pub output: Vec<ResponsesOutput>,
}

#[derive(Deserialize)]
pub(crate) struct ResponsesIncompleteDetails {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ResponsesOutput {
    #[serde(rename = "message")]
    Message { content: Vec<ResponsesContent> },
    #[serde(rename = "function_call")]
    FunctionCall { id: Option<String>, call_id: Option<String>, name: String, arguments: String },
    #[serde(rename = "reasoning")]
    Reasoning {
        #[serde(default)]
        summary: Vec<ReasoningSummaryPart>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ResponsesContent {
    #[serde(rename = "output_text")]
    OutputText { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ReasoningSummaryPart {
    #[serde(rename = "summary_text")]
    SummaryText { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub(crate) struct ChatCompletionsResponse {
    pub choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
pub(crate) struct ChatCompletionChoice {
    pub message: ChatCompletionMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ChatCompletionMessage {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ChatCompletionToolCall>,
}

#[derive(Deserialize)]
pub(crate) struct ChatCompletionToolCall {
    pub id: Option<String>,
    pub function: ChatCompletionFunction,
}

#[derive(Deserialize)]
pub(crate) struct ChatCompletionFunction {
    pub name: String,
    pub arguments: String,
}
