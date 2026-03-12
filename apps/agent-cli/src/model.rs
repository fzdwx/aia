use agent_core::{
    Completion, CompletionRequest, CoreError, LanguageModel, ModelDisposition, ModelIdentity,
    StreamEvent, ToolCall, ToolDefinition, ToolExecutor, ToolResult,
};
use openai_adapter::{OpenAiResponsesConfig, OpenAiResponsesModel};
use provider_registry::ProviderProfile;

use crate::errors::{CliModelError, CliSetupError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderLaunchChoice {
    Bootstrap,
    OpenAi(ProviderProfile),
}

pub enum CliModel {
    Bootstrap(BootstrapModel),
    OpenAi(OpenAiResponsesModel),
}

impl LanguageModel for CliModel {
    type Error = CliModelError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        match self {
            Self::Bootstrap(model) => model.complete(request).map_err(CliModelError::Bootstrap),
            Self::OpenAi(model) => model.complete(request).map_err(CliModelError::OpenAi),
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
            Self::OpenAi(model) => {
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
            let config =
                OpenAiResponsesConfig::new(profile.base_url, profile.api_key, profile.model);
            let model = OpenAiResponsesModel::new(config).map_err(CliSetupError::OpenAiAdapter)?;
            Ok((identity, CliModel::OpenAi(model)))
        }
    }
}

pub struct BootstrapModel;

impl LanguageModel for BootstrapModel {
    type Error = CoreError;

    fn complete(&self, request: CompletionRequest) -> Result<Completion, Self::Error> {
        let latest = request
            .conversation
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_else(|| "空输入".into());

        let tool_names = request
            .available_tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>()
            .join("、");

        Ok(Completion {
            segments: vec![
                agent_core::CompletionSegment::Text(format!(
                    "收到需求：{latest}。当前骨架已具备真实模型适配起点，下一步应优先推进统一工具规范的外部映射、MCP 接入与终端事件流，而不是直接堆完整界面。当前预留工具：{tool_names}。"
                )),
                agent_core::CompletionSegment::ToolUse(ToolCall::new("search_code")),
            ],
        })
    }
}

pub struct BootstrapTools;

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

    fn call(&self, call: &ToolCall) -> Result<ToolResult, Self::Error> {
        Ok(ToolResult::from_call(call, "起步阶段尚未接入真实工具执行器"))
    }
}
