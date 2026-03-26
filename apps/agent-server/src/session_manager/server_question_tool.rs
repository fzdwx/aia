use agent_core::{
    CoreError, QuestionItem, QuestionKind, QuestionOption, QuestionRequest, QuestionResult, Tool,
    ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::session_manager::{RuntimeWorkerError, types::{QuestionCoordinator, SessionCommand}};

#[derive(Clone)]
pub(crate) struct ServerQuestionTool {
    question_coordinator: std::sync::Arc<QuestionCoordinator>,
}

impl ServerQuestionTool {
    pub(crate) fn new(question_coordinator: std::sync::Arc<QuestionCoordinator>) -> Self {
        Self { question_coordinator }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolArgs {
    #[tool_schema(description = "Structured questions to show to the user.")]
    pub(crate) questions: Vec<QuestionToolQuestionItemArgs>,
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolQuestionItemArgs {
    pub(crate) id: String,
    pub(crate) question: String,
    #[tool_schema(description = "Question kind: choice, text, or confirm.")]
    pub(crate) kind: String,
    pub(crate) required: Option<bool>,
    pub(crate) multi_select: Option<bool>,
    pub(crate) options: Option<Vec<QuestionToolQuestionOptionArgs>>,
    pub(crate) placeholder: Option<String>,
    pub(crate) recommended_option_id: Option<String>,
    pub(crate) recommendation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestionToolQuestionOptionArgs {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) description: Option<String>,
}

impl QuestionToolArgs {
    pub(crate) fn into_request(
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
impl Tool for ServerQuestionTool {
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

        let session_id = context
            .session_id
            .clone()
            .ok_or_else(|| CoreError::new("missing session_id in tool execution context"))?;

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.question_coordinator
            .tx
            .send(SessionCommand::AskQuestion {
                session_id,
                request: request.clone(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| CoreError::new("question coordinator unavailable"))?;

        let result: QuestionResult = reply_rx
            .await
            .map_err(|_| CoreError::new("question coordinator dropped"))?
            .map_err(|error: RuntimeWorkerError| CoreError::new(error.message))?;

        let details = serde_json::to_value(&result)
            .map_err(|error| CoreError::new(format!("failed to serialize QuestionResult: {error}")))?;
        let content = serde_json::to_string(&result)
            .map_err(|error| CoreError::new(format!("failed to encode QuestionResult: {error}")))?;
        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}
