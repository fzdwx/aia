use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{
    CoreError, PendingToolRequest, QuestionItem, QuestionKind, QuestionOption, QuestionRequest,
    Tool, ToolCall, ToolCallOutcome, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::tool_descriptions::question_tool_description;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct QuestionTool;

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolArgs {
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
        request_id: String,
    ) -> Result<QuestionRequest, CoreError> {
        Ok(QuestionRequest {
            request_id,
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

fn fallback_question_request_id() -> String {
    static NEXT_QUESTION_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_QUESTION_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    format!("qreq_{now_ms}_{id}")
}

pub fn question_request_from_call(
    call: &ToolCall,
    turn_id: &str,
    request_id: impl Into<String>,
) -> Result<QuestionRequest, CoreError> {
    let args: QuestionToolArgs = call.parse_arguments()?;
    args.into_request(call, turn_id, request_id.into())
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

    fn is_interactive_tool(&self) -> bool {
        true
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolCallOutcome, CoreError> {
        let request =
            question_request_from_call(tool_call, &context.run_id, fallback_question_request_id())?;
        let payload = serde_json::to_value(&request).map_err(|error| {
            CoreError::new(format!("failed to serialize QuestionRequest: {error}"))
        })?;
        Ok(ToolCallOutcome::suspended(PendingToolRequest {
            request_id: request.request_id.clone(),
            invocation_id: request.invocation_id.clone(),
            turn_id: request.turn_id.clone(),
            tool_name: tool_call.tool_name.clone(),
            kind: "question".into(),
            payload,
        }))
    }
}
