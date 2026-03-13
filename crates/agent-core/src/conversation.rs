use serde::{Deserialize, Serialize};

use crate::{ToolCall, ToolResult};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConversationItem {
    Message(Message),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

impl ConversationItem {
    pub fn message(role: Role, content: impl Into<String>) -> Self {
        Self::Message(Message::new(role, content))
    }

    pub fn as_message(&self) -> Option<&Message> {
        match self {
            Self::Message(message) => Some(message),
            Self::ToolCall(_) | Self::ToolResult(_) => None,
        }
    }

    pub fn as_tool_call(&self) -> Option<&ToolCall> {
        match self {
            Self::ToolCall(call) => Some(call),
            Self::Message(_) | Self::ToolResult(_) => None,
        }
    }

    pub fn as_tool_result(&self) -> Option<&ToolResult> {
        match self {
            Self::ToolResult(result) => Some(result),
            Self::Message(_) | Self::ToolCall(_) => None,
        }
    }
}
