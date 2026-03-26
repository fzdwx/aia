use std::sync::{Arc, Mutex};

use agent_core::{
    CoreError, LanguageModel, QuestionItem, QuestionKind, QuestionOption, QuestionRequest,
    RuntimeToolContext, RuntimeToolContextStats, SessionInteractionCapabilities, Tool, ToolCall,
    ToolDefinition, ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolRegistry, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::{tape_handoff_tool_description, tape_info_tool_description};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AgentRuntime, helpers::next_question_request_id};

pub(super) fn build_runtime_tool_registry(
    capabilities: &SessionInteractionCapabilities,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(TapeInfoTool));
    registry.register(Box::new(TapeHandoffTool));
    if capabilities.can_use_question_tool() {
        registry.register(Box::new(QuestionTool));
    }
    registry
}

pub(super) struct RuntimeToolContextBridge {
    total_entries: usize,
    anchor_count: usize,
    entries_since_last_anchor: usize,
    last_input_tokens: Option<u32>,
    context_limit: Option<u32>,
    output_limit: Option<u32>,
    pressure_ratio: Option<f64>,
    pending_handoffs: Mutex<Vec<(String, String)>>,
}

impl RuntimeToolContextBridge {
    pub(super) fn new<M, T>(runtime: &AgentRuntime<M, T>) -> Arc<Self>
    where
        M: LanguageModel,
        T: ToolExecutor,
    {
        let stats = runtime.context_stats();
        Arc::new(Self {
            total_entries: stats.total_entries,
            anchor_count: stats.anchor_count,
            entries_since_last_anchor: stats.entries_since_last_anchor,
            last_input_tokens: stats.last_input_tokens,
            context_limit: stats.context_limit,
            output_limit: stats.output_limit,
            pressure_ratio: stats.pressure_ratio,
            pending_handoffs: Mutex::new(Vec::new()),
        })
    }

    pub(super) fn drain_handoffs(&self) -> Vec<(String, String)> {
        let mut guard =
            self.pending_handoffs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *guard)
    }
}

#[async_trait]
impl RuntimeToolContext for RuntimeToolContextBridge {
    fn context_stats(&self) -> RuntimeToolContextStats {
        RuntimeToolContextStats {
            total_entries: self.total_entries,
            anchor_count: self.anchor_count,
            entries_since_last_anchor: self.entries_since_last_anchor,
            last_input_tokens: self.last_input_tokens,
            context_limit: self.context_limit,
            output_limit: self.output_limit,
            pressure_ratio: self.pressure_ratio,
        }
    }

    fn record_handoff(&self, name: &str, summary: &str) -> Result<(), CoreError> {
        self.pending_handoffs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((name.to_string(), summary.to_string()));
        Ok(())
    }
}

struct TapeInfoTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct TapeInfoToolArgs {}

#[async_trait]
impl Tool for TapeInfoTool {
    fn name(&self) -> &str {
        "TapeInfo"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), tape_info_tool_description())
            .with_parameters_schema::<TapeInfoToolArgs>()
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let _: TapeInfoToolArgs = tool_call.parse_arguments()?;
        let runtime = context
            .runtime
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool context unavailable"))?;
        let stats = runtime.context_stats();
        let details = json!({
            "entries": stats.total_entries,
            "anchors": stats.anchor_count,
            "entries_since_last_anchor": stats.entries_since_last_anchor,
            "last_input_tokens": stats.last_input_tokens,
            "context_limit": stats.context_limit,
            "output_limit": stats.output_limit,
            "pressure_ratio": stats.pressure_ratio,
        });
        let content = serde_json::to_string_pretty(&details)
            .map_err(|error| CoreError::new(format!("failed to serialize TapeInfo: {error}")))?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}

struct TapeHandoffTool;

struct QuestionTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct TapeHandoffToolArgs {
    #[tool_schema(description = "A concise summary of the conversation so far to carry forward.")]
    summary: String,
    #[tool_schema(description = "Optional name for the anchor (default: \"handoff\").")]
    name: Option<String>,
}

#[async_trait]
impl Tool for TapeHandoffTool {
    fn name(&self) -> &str {
        "TapeHandoff"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), tape_handoff_tool_description())
            .with_parameters_schema::<TapeHandoffToolArgs>()
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: TapeHandoffToolArgs = tool_call.parse_arguments()?;
        let runtime = context
            .runtime
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool context unavailable"))?;
        let name = args.name.as_deref().unwrap_or("handoff");

        runtime.record_handoff(name, &args.summary)?;
        Ok(ToolResult::from_call(tool_call, format!("anchor added: {name}")))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct QuestionToolArgs {
    #[tool_schema(description = "Structured questions to show to the user.")]
    questions: Vec<QuestionToolQuestionItemArgs>,
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct QuestionToolQuestionItemArgs {
    id: String,
    question: String,
    #[tool_schema(description = "Question kind: choice, text, or confirm.")]
    kind: String,
    required: Option<bool>,
    multi_select: Option<bool>,
    options: Option<Vec<QuestionToolQuestionOptionArgs>>,
    placeholder: Option<String>,
    recommended_option_id: Option<String>,
    recommendation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
struct QuestionToolQuestionOptionArgs {
    id: String,
    label: String,
    description: Option<String>,
}

impl QuestionToolArgs {
    fn into_request(
        self,
        tool_call: &ToolCall,
        turn_id: &str,
    ) -> Result<QuestionRequest, CoreError> {
        Ok(QuestionRequest {
            request_id: next_question_request_id(),
            invocation_id: tool_call.invocation_id.clone(),
            turn_id: turn_id.to_string(),
            questions: self
                .questions
                .into_iter()
                .map(QuestionItem::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<QuestionToolQuestionItemArgs> for QuestionItem {
    type Error = CoreError;

    fn try_from(value: QuestionToolQuestionItemArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            question: value.question,
            kind: parse_question_kind(&value.kind)?,
            required: value.required.unwrap_or(false),
            multi_select: value.multi_select.unwrap_or(false),
            options: value
                .options
                .unwrap_or_default()
                .into_iter()
                .map(QuestionOption::from)
                .collect(),
            placeholder: value.placeholder,
            recommended_option_id: value.recommended_option_id,
            recommendation_reason: value.recommendation_reason,
        })
    }
}

impl From<QuestionToolQuestionOptionArgs> for QuestionOption {
    fn from(value: QuestionToolQuestionOptionArgs) -> Self {
        Self { id: value.id, label: value.label, description: value.description }
    }
}

fn parse_question_kind(raw: &str) -> Result<QuestionKind, CoreError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "choice" => Ok(QuestionKind::Choice),
        "text" => Ok(QuestionKind::Text),
        "confirm" => Ok(QuestionKind::Confirm),
        other => Err(CoreError::new(format!(
            "invalid Question kind: {other}; expected choice, text, or confirm"
        ))),
    }
}

#[async_trait]
impl Tool for QuestionTool {
    fn name(&self) -> &str {
        "Question"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            self.name(),
            agent_prompts::tool_descriptions::question_tool_description(),
        )
        .with_parameters_schema::<QuestionToolArgs>()
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let request = tool_call
            .parse_arguments::<QuestionToolArgs>()?
            .into_request(tool_call, &context.run_id)?;
        let details = serde_json::to_value(&request).map_err(|error| {
            CoreError::new(format!("failed to serialize QuestionRequest: {error}"))
        })?;
        let content = serde_json::to_string(&request).map_err(|error| {
            CoreError::new(format!("failed to encode QuestionRequest: {error}"))
        })?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}

pub(super) fn is_question_tool_call(call: &ToolCall) -> bool {
    call.tool_name == "Question"
}

pub(super) fn question_request_from_call(
    call: &ToolCall,
    turn_id: &str,
) -> Result<QuestionRequest, CoreError> {
    let args: QuestionToolArgs = call.parse_arguments()?;
    args.into_request(call, turn_id)
}

pub(super) fn runtime_tool_definitions() -> Vec<ToolDefinition> {
    runtime_tool_definitions_for(&SessionInteractionCapabilities::interactive())
}

pub(super) fn is_runtime_tool(name: &str) -> bool {
    runtime_tool_definitions().iter().any(|definition| definition.name == name)
}

pub(super) fn runtime_tool_definitions_for(
    capabilities: &SessionInteractionCapabilities,
) -> Vec<ToolDefinition> {
    build_runtime_tool_registry(capabilities).definitions()
}

#[cfg(test)]
#[path = "../../tests/runtime/tape_tools/mod.rs"]
mod tests;
