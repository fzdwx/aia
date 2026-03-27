use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::{
    CoreError, PendingToolRequest, QUESTION_INTERACTION_KIND, QuestionAnswer, QuestionItem,
    QuestionKind, QuestionOption, QuestionRequest, QuestionResult, QuestionResultStatus, Tool,
    ToolCall, ToolCallOutcome, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
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

pub fn question_tool_call(request: &QuestionRequest) -> ToolCall {
    ToolCall::new("Question")
        .with_invocation_id(request.invocation_id.clone())
        .with_arguments_value(serde_json::json!({ "questions": request.questions }))
}

pub fn question_tool_result_from_request(
    request: &QuestionRequest,
    result: &QuestionResult,
) -> Result<ToolResult, CoreError> {
    validate_question_result(request, result)?;
    let call = question_tool_call(request);
    let content = serde_json::to_string(result)
        .map_err(|error| CoreError::new(format!("failed to encode QuestionResult: {error}")))?;
    let details = serde_json::to_value(result)
        .map_err(|error| CoreError::new(format!("failed to serialize QuestionResult: {error}")))?;
    Ok(ToolResult::from_call(&call, content).with_details(details))
}

fn validate_question_result(
    request: &QuestionRequest,
    result: &QuestionResult,
) -> Result<(), CoreError> {
    if request.request_id != result.request_id {
        return Err(CoreError::new(format!(
            "question result request_id mismatch: expected {}, got {}",
            request.request_id, result.request_id
        )));
    }

    let questions_by_id = request
        .questions
        .iter()
        .map(|question| (question.id.as_str(), question))
        .collect::<BTreeMap<_, _>>();

    let mut answered_question_ids = BTreeSet::new();
    for answer in &result.answers {
        let question = questions_by_id.get(answer.question_id.as_str()).ok_or_else(|| {
            CoreError::new(format!(
                "question result references unknown question_id: {}",
                answer.question_id
            ))
        })?;

        if !answered_question_ids.insert(answer.question_id.as_str()) {
            return Err(CoreError::new(format!(
                "question result contains duplicate answers for question_id: {}",
                answer.question_id
            )));
        }

        validate_answer_against_question(answer, question)?;
    }

    if result.status == QuestionResultStatus::Answered {
        for question in request.questions.iter().filter(|question| question.required) {
            let Some(answer) =
                result.answers.iter().find(|answer| answer.question_id == question.id)
            else {
                return Err(CoreError::new(format!(
                    "missing required answer for question_id: {}",
                    question.id
                )));
            };

            if !answer_has_value(answer) {
                return Err(CoreError::new(format!(
                    "required question answer is empty for question_id: {}",
                    question.id
                )));
            }
        }
    }

    Ok(())
}

fn validate_answer_against_question(
    answer: &QuestionAnswer,
    question: &QuestionItem,
) -> Result<(), CoreError> {
    match question.kind {
        QuestionKind::Choice | QuestionKind::Confirm => {
            if answer.text.is_some() {
                return Err(CoreError::new(format!(
                    "question_id {} does not accept freeform text answers",
                    question.id
                )));
            }
            if !question.multi_select && answer.selected_option_ids.len() > 1 {
                return Err(CoreError::new(format!(
                    "question_id {} does not allow multiple selections",
                    question.id
                )));
            }
            let allowed_option_ids =
                question.options.iter().map(|option| option.id.as_str()).collect::<BTreeSet<_>>();
            for option_id in &answer.selected_option_ids {
                if !allowed_option_ids.contains(option_id.as_str()) {
                    return Err(CoreError::new(format!(
                        "question_id {} references unknown option_id: {}",
                        question.id, option_id
                    )));
                }
            }
        }
        QuestionKind::Text => {
            if !answer.selected_option_ids.is_empty() {
                return Err(CoreError::new(format!(
                    "question_id {} does not accept selected options",
                    question.id
                )));
            }
        }
    }

    Ok(())
}

fn answer_has_value(answer: &QuestionAnswer) -> bool {
    !answer.selected_option_ids.is_empty()
        || answer.text.as_ref().is_some_and(|text| !text.trim().is_empty())
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

    fn interactive_kind(&self) -> Option<&str> {
        Some(QUESTION_INTERACTION_KIND)
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
