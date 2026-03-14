use std::fmt;

use agent_core::{CompletionStopReason, ToolCall, ToolResult};

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

    pub fn session(error: impl fmt::Display) -> Self {
        Self { message: format!("会话持久化失败：{error}") }
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

    pub fn stop_reason_mismatch(stop_reason: &CompletionStopReason) -> Self {
        Self { message: format!("停止原因与完成内容不匹配：{stop_reason:?}") }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RuntimeError {}
