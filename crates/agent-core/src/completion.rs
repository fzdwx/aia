use serde::{Deserialize, Serialize};

use crate::{ConversationItem, ToolCall, ToolDefinition};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModelDisposition {
    Balanced,
    Precise,
    Fast,
    Creative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelLimit {
    pub context: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub fn as_api_value(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "minimal" => Some(Self::Minimal),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::Xhigh),
            _ => None,
        }
    }

    pub fn parse_optional(value: Option<&str>) -> Result<Option<Self>, String> {
        value
            .map(|value| {
                Self::parse(value).ok_or_else(|| format!("invalid reasoning_effort: {value}"))
            })
            .transpose()
    }

    pub fn normalize(value: Option<String>) -> Option<String> {
        value.and_then(|value| Self::parse(&value).map(|effort| effort.as_api_value().to_string()))
    }

    pub fn parse_persisted(value: Option<String>) -> Option<Self> {
        value.and_then(|value| Self::parse(&value))
    }

    pub fn normalize_for_model(value: Option<String>, supports_reasoning: bool) -> Option<Self> {
        if !supports_reasoning {
            return None;
        }
        Self::parse_persisted(value)
    }

    pub fn serialize_optional(value: Option<Self>) -> Option<String> {
        value.map(|effort| effort.as_api_value().to_string())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub provider: String,
    pub name: String,
    pub disposition: ModelDisposition,
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(default)]
    pub limit: Option<ModelLimit>,
}

impl ModelIdentity {
    pub fn new(
        provider: impl Into<String>,
        name: impl Into<String>,
        disposition: ModelDisposition,
    ) -> Self {
        Self {
            provider: provider.into(),
            name: name.into(),
            disposition,
            reasoning_effort: None,
            limit: None,
        }
    }

    pub fn with_reasoning_effort(mut self, effort: Option<ReasoningEffort>) -> Self {
        self.reasoning_effort = effort;
        self
    }

    pub fn with_limit(mut self, limit: Option<ModelLimit>) -> Self {
        self.limit = limit;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompletionSegment {
    Text(String),
    Thinking(String),
    ToolUse(ToolCall),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompletionStopReason {
    #[default]
    Stop,
    ToolUse,
    MaxTokens,
    ContentFilter,
    Unknown(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompletionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptCacheRetention {
    FiveMinutes,
    OneHour,
    OneDay,
}

impl PromptCacheRetention {
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Self::FiveMinutes => "5m",
            Self::OneHour => "1h",
            Self::OneDay => "24h",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptCacheConfig {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub retention: Option<PromptCacheRetention>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequestTimeoutConfig {
    #[serde(default)]
    pub read_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub segments: Vec<CompletionSegment>,
    #[serde(default)]
    pub stop_reason: CompletionStopReason,
    #[serde(default)]
    pub usage: Option<CompletionUsage>,
    #[serde(default)]
    pub response_body: Option<String>,
    #[serde(default)]
    pub http_status_code: Option<u16>,
}

impl Completion {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            segments: vec![CompletionSegment::Text(content.into())],
            stop_reason: CompletionStopReason::Stop,
            usage: None,
            response_body: None,
            http_status_code: None,
        }
    }

    pub fn plain_text(&self) -> String {
        self.segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::Text(text) => Some(text.as_str()),
                CompletionSegment::Thinking(_) | CompletionSegment::ToolUse(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn thinking_text(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .segments
            .iter()
            .filter_map(|segment| match segment {
                CompletionSegment::Thinking(text) => Some(text.as_str()),
                CompletionSegment::Text(_) | CompletionSegment::ToolUse(_) => None,
            })
            .collect();
        if parts.is_empty() { None } else { Some(parts.join("")) }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceRequestContext {
    pub session_id: Option<String>,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub root_span_id: String,
    pub operation_name: String,
    pub turn_id: String,
    pub run_id: String,
    pub request_kind: String,
    pub step_index: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: ModelIdentity,
    pub instructions: Option<String>,
    pub conversation: Vec<ConversationItem>,
    pub max_output_tokens: Option<u32>,
    pub available_tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub prompt_cache: Option<PromptCacheConfig>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub timeout: Option<RequestTimeoutConfig>,
    pub trace_context: Option<LlmTraceRequestContext>,
}
