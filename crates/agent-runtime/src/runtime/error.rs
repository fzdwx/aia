use std::fmt;

use agent_core::{ToolCall, ToolResult};

use super::helpers::PreviousToolCall;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeError {
    message: String,
}

impl RuntimeError {
    pub fn model(error: impl fmt::Display) -> Self {
        Self { message: format!("模型执行失败：{error}") }
    }

    pub fn subscription(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub fn tool(error: impl fmt::Display) -> Self {
        Self { message: format!("工具执行失败：{error}") }
    }

    pub fn tool_unavailable(tool_name: impl Into<String>) -> Self {
        Self { message: format!("工具不可用：{}", tool_name.into()) }
    }

    pub fn tool_result_mismatch(call: &ToolCall, result: &ToolResult) -> Self {
        Self {
            message: format!(
                "工具结果不匹配：调用 {}#{}, 结果 {}#{}",
                call.tool_name, call.invocation_id, result.tool_name, result.invocation_id
            ),
        }
    }

    pub fn tool_call_limit(max_tool_calls: usize) -> Self {
        Self { message: format!("轮次超过最大工具调用次数：{max_tool_calls}") }
    }

    pub(super) fn duplicate_tool_call(call: &ToolCall, previous: &PreviousToolCall) -> Self {
        Self {
            message: format!(
                "重复工具调用已跳过：{}#{} 在本轮内已用相同参数执行过。请直接基于已有结果继续。上次结果：{}",
                call.tool_name, call.invocation_id, previous.summary
            ),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RuntimeError {}
