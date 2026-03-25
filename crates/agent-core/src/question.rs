use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    Choice,
    Text,
    Confirm,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionItem {
    pub id: String,
    pub header: String,
    pub question: String,
    pub kind: QuestionKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<QuestionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_option_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionRequest {
    pub request_id: String,
    pub invocation_id: String,
    pub turn_id: String,
    pub questions: Vec<QuestionItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub question_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_option_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionResultStatus {
    Answered,
    Cancelled,
    Dismissed,
    TimedOut,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionResult {
    pub status: QuestionResultStatus,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<QuestionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

