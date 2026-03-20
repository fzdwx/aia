mod dashboard;
mod mapping;
mod schema;
mod store;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmTraceStatus {
    Succeeded,
    Failed,
}

impl LlmTraceStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "failed" => Self::Failed,
            _ => Self::Succeeded,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LlmTraceSpanKind {
    Client,
    Internal,
}

impl LlmTraceSpanKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Client => "CLIENT",
            Self::Internal => "INTERNAL",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "INTERNAL" | "internal" => Self::Internal,
            _ => Self::Client,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceEvent {
    pub name: String,
    pub at_ms: u64,
    pub attributes: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceRecord {
    pub id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub root_span_id: String,
    pub operation_name: String,
    pub span_kind: LlmTraceSpanKind,
    pub session_id: Option<String>,
    pub turn_id: String,
    pub run_id: String,
    pub request_kind: String,
    pub step_index: u32,
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub base_url: String,
    pub endpoint_path: String,
    pub streaming: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub status: LlmTraceStatus,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
    pub request_summary: Value,
    pub provider_request: Value,
    pub response_summary: Value,
    pub response_body: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub otel_attributes: Value,
    pub events: Vec<LlmTraceEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceListItem {
    pub id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub root_span_id: String,
    pub operation_name: String,
    pub span_kind: LlmTraceSpanKind,
    pub turn_id: String,
    pub run_id: String,
    pub request_kind: String,
    pub step_index: u32,
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub endpoint_path: String,
    pub status: LlmTraceStatus,
    pub stop_reason: Option<String>,
    pub status_code: Option<u16>,
    pub started_at_ms: u64,
    pub duration_ms: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub user_message: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceSummary {
    pub total_requests: u64,
    pub failed_requests: u64,
    pub partial_requests: u64,
    pub avg_duration_ms: Option<f64>,
    pub p95_duration_ms: Option<u64>,
    pub total_llm_spans: u64,
    pub total_tool_spans: u64,
    pub requests_with_tools: u64,
    pub failed_tool_calls: u64,
    pub unique_models: u64,
    pub latest_request_started_at_ms: Option<u64>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub total_cached_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmTraceLoopStatus {
    Completed,
    Failed,
    Partial,
}

impl LlmTraceLoopStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Partial => "partial",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "failed" => Self::Failed,
            "partial" => Self::Partial,
            _ => Self::Completed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceLoopItem {
    pub id: String,
    pub trace_id: String,
    pub request_kind: String,
    pub turn_id: String,
    pub run_id: String,
    pub root_span_id: String,
    pub model: String,
    pub protocol: String,
    pub endpoint_path: String,
    pub latest_started_at_ms: u64,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub total_tokens: u64,
    pub total_cached_tokens: u64,
    pub llm_span_count: u32,
    pub tool_span_count: u32,
    pub failed_tool_count: u32,
    pub final_status: LlmTraceLoopStatus,
    pub user_message: Option<String>,
    pub latest_error: Option<String>,
    pub final_span_id: Option<String>,
    pub traces: Vec<LlmTraceListItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceLoopPage {
    pub items: Vec<LlmTraceLoopItem>,
    pub total_items: u64,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceLoopDetail {
    pub loop_item: LlmTraceLoopItem,
    pub trace_details: Vec<LlmTraceRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceOverview {
    pub summary: LlmTraceSummary,
    pub page: LlmTraceLoopPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmTraceDashboardRange {
    Today,
    Week,
    Month,
}

impl LlmTraceDashboardRange {
    pub fn from_str(value: &str) -> Self {
        match value {
            "today" => Self::Today,
            "week" => Self::Week,
            _ => Self::Month,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceDashboardSummary {
    pub total_cost_usd: f64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub partial_requests: u64,
    pub total_sessions: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub total_cached_tokens: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub total_lines_changed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceDashboardTrendPoint {
    pub bucket_start_ms: u64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub partial_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cached_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceDashboardActivityPoint {
    pub day_start_ms: u64,
    pub total_requests: u64,
    pub total_sessions: u64,
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub total_lines_changed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceDashboard {
    pub range: LlmTraceDashboardRange,
    pub current: LlmTraceDashboardSummary,
    pub previous: LlmTraceDashboardSummary,
    pub trend: Vec<LlmTraceDashboardTrendPoint>,
    pub activity: Vec<LlmTraceDashboardActivityPoint>,
    pub overall_summary: LlmTraceSummary,
    pub conversation_summary: LlmTraceSummary,
    pub compression_summary: LlmTraceSummary,
}

pub trait LlmTraceStore: Send + Sync {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), crate::AiaStoreError>;
    fn get(&self, id: &str) -> Result<Option<LlmTraceRecord>, crate::AiaStoreError>;
    fn summary(&self) -> Result<LlmTraceSummary, crate::AiaStoreError>;
}
