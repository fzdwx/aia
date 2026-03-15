use agent_core::{LanguageModel, ToolCall, ToolDefinition, ToolExecutor, ToolResult};
use serde_json::json;

use super::{AgentRuntime, RuntimeError};

pub(super) fn tape_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::new(
            "tape.info",
            "Return context usage statistics for the current session.",
        ),
        ToolDefinition::new(
            "tape.handoff",
            "Create an anchor to truncate history and carry forward a summary as minimal inherited state.",
        )
        .with_parameter("summary", "A concise summary of the conversation so far to carry forward.", true)
        .with_parameter("name", "Optional name for the anchor (default: \"handoff\").", false),
    ]
}

pub(super) fn is_tape_tool(name: &str) -> bool {
    matches!(name, "tape.info" | "tape.handoff")
}

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn execute_tape_tool(
        &mut self,
        call: &ToolCall,
    ) -> Result<ToolResult, RuntimeError> {
        let content = match call.tool_name.as_str() {
            "tape.info" => {
                let stats = self.context_stats();
                format!(
                    "entries: {}\n\
                     anchors: {}\n\
                     entries_since_last_anchor: {}\n\
                     estimated_context_units: {}\n\
                     context_limit: {}\n\
                     pressure_ratio: {:.2}",
                    stats.total_entries,
                    stats.anchor_count,
                    stats.entries_since_last_anchor,
                    stats.estimated_context_units,
                    stats.context_limit.map_or("unknown".to_string(), |v| v.to_string()),
                    stats.pressure_ratio.unwrap_or(0.0),
                )
            }
            "tape.handoff" => {
                let summary = call
                    .arguments
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = call
                    .arguments
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("handoff")
                    .to_string();
                self.record_handoff(&name, json!({ "summary": summary }), "ai")?;
                format!("anchor added: {name}")
            }
            _ => return Err(RuntimeError::tool_unavailable(call.tool_name.clone())),
        };
        Ok(ToolResult::from_call(call, content))
    }
}
