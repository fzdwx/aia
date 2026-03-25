use agent_core::{
    CoreError, QuestionItem, QuestionKind, QuestionOption, QuestionRequest, Tool, ToolCall,
    ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::helpers::next_question_request_id;

pub(super) struct QuestionTool;

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct QuestionToolArgs {
    #[tool_schema(description = "Structured questions to show to the user.")]
    pub(super) questions: Vec<QuestionToolQuestionItemArgs>,
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct QuestionToolQuestionItemArgs {
    pub(super) id: String,
    pub(super) header: String,
    pub(super) question: String,
    #[tool_schema(description = "Question kind: choice, text, or confirm.")]
    pub(super) kind: String,
    pub(super) required: Option<bool>,
    pub(super) multi_select: Option<bool>,
    pub(super) options: Option<Vec<QuestionToolQuestionOptionArgs>>,
    pub(super) placeholder: Option<String>,
    pub(super) recommended_option_ids: Option<Vec<String>>,
    pub(super) recommendation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct QuestionToolQuestionOptionArgs {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) description: Option<String>,
}

impl QuestionToolArgs {
    pub(super) fn into_request(
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
            header: value.header,
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
            recommended_option_ids: value.recommended_option_ids.unwrap_or_default(),
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
        let request = question_request_from_call(tool_call, &context.run_id)?;
        let details = serde_json::to_value(&request).map_err(|error| {
            CoreError::new(format!("failed to serialize QuestionRequest: {error}"))
        })?;
        let content = serde_json::to_string(&request).map_err(|error| {
            CoreError::new(format!("failed to encode QuestionRequest: {error}"))
        })?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}
