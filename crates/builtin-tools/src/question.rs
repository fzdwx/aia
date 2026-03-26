use agent_core::{
    CoreError, QuestionItem, QuestionKind, QuestionOption, QuestionRequest, Tool, ToolCall,
    ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::question_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct QuestionTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolArgs {
    #[tool_schema(description = "Structured questions to show to the user.")]
    questions: Vec<QuestionToolQuestionItemArgs>,
}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolQuestionItemArgs {
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

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolQuestionOptionArgs {
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
            request_id: format!(
                "qreq_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
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
        ToolDefinition::new(self.name(), question_tool_description())
            .with_parameters_schema::<QuestionToolArgs>()
    }

    fn requires_interactive_capability(&self) -> bool {
        true
    }

    fn requires_runtime_context(&self) -> bool {
        true
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let runtime_host = context
            .runtime_host
            .as_ref()
            .ok_or_else(|| CoreError::new("runtime tool host unavailable"))?;
        let session_id = context
            .session_id
            .as_deref()
            .ok_or_else(|| CoreError::new("missing session_id in tool execution context"))?;
        let request = tool_call
            .parse_arguments::<QuestionToolArgs>()?
            .into_request(tool_call, &context.run_id)?;
        let result = runtime_host.ask_question(session_id, request).await?;
        let details = serde_json::to_value(&result).map_err(|error| {
            CoreError::new(format!("failed to serialize QuestionResult: {error}"))
        })?;
        let content = serde_json::to_string(&result)
            .map_err(|error| CoreError::new(format!("failed to encode QuestionResult: {error}")))?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}
