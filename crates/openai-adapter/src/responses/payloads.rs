use serde::Deserialize;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ResponsesResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub usage: Option<ResponsesUsage>,
    #[serde(default)]
    pub incomplete_details: Option<ResponsesIncompleteDetails>,
    pub output: Vec<ResponsesOutput>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ResponsesUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub input_tokens_details: Option<ResponsesInputTokensDetails>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ResponsesInputTokensDetails {
    #[serde(default)]
    pub cached_tokens: Option<u64>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
pub(crate) struct ResponsesIncompleteDetails {
    #[serde(default)]
    pub reason: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ResponsesContent {
    #[serde(rename = "output_text")]
    OutputText { text: String },
    #[serde(other)]
    Other,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ReasoningSummaryPart {
    #[serde(rename = "summary_text")]
    SummaryText { text: String },
    #[serde(other)]
    Other,
}
