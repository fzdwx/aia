use agent_core::{
    Completion, CompletionRequest, CoreError, LanguageModel, ModelDisposition, ModelIdentity, Role,
    StreamEvent, ToolCall,
};
#[cfg(test)]
use agent_core::{ToolDefinition, ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolResult};
use openai_adapter::{
    OpenAiChatCompletionsConfig, OpenAiChatCompletionsModel, OpenAiResponsesConfig,
    OpenAiResponsesModel,
};
use provider_registry::{ProviderKind, ProviderProfile};

use crate::errors::{CliModelError, CliSetupError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderLaunchChoice {
    Bootstrap,
    OpenAi(ProviderProfile),
}

pub enum CliModel {
    Bootstrap(BootstrapModel),
    OpenAiResponses(OpenAiResponsesModel),
    OpenAiChatCompletions(OpenAiChatCompletionsModel),
}

impl LanguageModel for CliModel {
    type Error = CliModelError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        match self {
            Self::Bootstrap(model) => model.complete(request).map_err(CliModelError::Bootstrap),
            Self::OpenAiResponses(model) => model.complete(request).map_err(CliModelError::OpenAi),
            Self::OpenAiChatCompletions(model) => {
                model.complete(request).map_err(CliModelError::OpenAi)
            }
        }
    }

    fn complete_streaming(
        &self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<Completion, Self::Error> {
        match self {
            Self::Bootstrap(model) => {
                model.complete_streaming(request, sink).map_err(CliModelError::Bootstrap)
            }
            Self::OpenAiResponses(model) => {
                model.complete_streaming(request, sink).map_err(CliModelError::OpenAi)
            }
            Self::OpenAiChatCompletions(model) => {
                model.complete_streaming(request, sink).map_err(CliModelError::OpenAi)
            }
        }
    }
}

pub fn build_model_from_selection(
    selection: ProviderLaunchChoice,
) -> Result<(ModelIdentity, CliModel), CliSetupError> {
    match selection {
        ProviderLaunchChoice::Bootstrap => Ok((
            ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced),
            CliModel::Bootstrap(BootstrapModel),
        )),
        ProviderLaunchChoice::OpenAi(profile) => {
            let identity =
                ModelIdentity::new("openai", profile.model.clone(), ModelDisposition::Balanced);
            match profile.kind {
                ProviderKind::OpenAiResponses => {
                    let config = OpenAiResponsesConfig::new(
                        profile.base_url,
                        profile.api_key,
                        profile.model,
                    );
                    let model =
                        OpenAiResponsesModel::new(config).map_err(CliSetupError::OpenAiAdapter)?;
                    Ok((identity, CliModel::OpenAiResponses(model)))
                }
                ProviderKind::OpenAiChatCompletions => {
                    let config = OpenAiChatCompletionsConfig::new(
                        profile.base_url,
                        profile.api_key,
                        profile.model,
                    );
                    let model = OpenAiChatCompletionsModel::new(config)
                        .map_err(CliSetupError::OpenAiAdapter)?;
                    Ok((identity, CliModel::OpenAiChatCompletions(model)))
                }
            }
        }
    }
}

pub struct BootstrapModel;

impl LanguageModel for BootstrapModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let last_user_index = request
            .conversation
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| item.as_message().is_some_and(|message| message.role == Role::User))
            .map(|(index, _)| index);
        let saw_tool_result = last_user_index
            .map(|index| {
                request
                    .conversation
                    .iter()
                    .skip(index + 1)
                    .any(|item| item.as_tool_result().is_some())
            })
            .unwrap_or(false);
        let latest_user = request
            .conversation
            .iter()
            .rev()
            .find_map(|item| {
                item.as_message()
                    .filter(|message| message.role == Role::User)
                    .map(|message| message.content.clone())
            })
            .unwrap_or_else(|| "空输入".into());

        if saw_tool_result {
            let latest_tool = last_user_index
                .map(|index| {
                    request
                        .conversation
                        .iter()
                        .skip(index + 1)
                        .rev()
                        .find_map(|item| item.as_tool_result().map(|result| result.content.clone()))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            return Ok(Completion::text(format!(
                "收到需求：{latest_user}。当前骨架已具备真实模型适配起点，下一步应优先推进统一工具规范的外部映射、MCP 接入与终端事件流，而不是直接堆完整界面。最近工具结果：{latest_tool}。"
            )));
        }

        let tool_names = request
            .available_tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>()
            .join("、");

        Ok(Completion {
            segments: vec![
                agent_core::CompletionSegment::Text(format!(
                    "我会先查看可用工具并继续处理。当前预留工具：{tool_names}。"
                )),
                agent_core::CompletionSegment::ToolUse(ToolCall::new("search_code")),
            ],
            checkpoint: None,
        })
    }
}

#[cfg(test)]
pub struct BootstrapTools;

#[cfg(test)]
impl ToolExecutor for BootstrapTools {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new("search_code", "搜索工作区代码").with_parameter(
                "query",
                "搜索关键字",
                true,
            ),
            ToolDefinition::new("handoff_session", "生成交接摘要").with_parameter(
                "summary",
                "交接摘要",
                true,
            ),
        ]
    }

    fn call(
        &self,
        call: &ToolCall,
        _output: &mut dyn FnMut(ToolOutputDelta),
        _context: &ToolExecutionContext,
    ) -> Result<ToolResult, Self::Error> {
        Ok(ToolResult::from_call(call, "起步阶段尚未接入真实工具执行器"))
    }
}
