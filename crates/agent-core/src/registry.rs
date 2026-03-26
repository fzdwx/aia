use std::collections::BTreeMap;

use async_trait::async_trait;

use crate::{
    CoreError, SessionInteractionCapabilities, Tool, ToolCall, ToolDefinition,
    ToolExecutionContext, ToolExecutor, ToolOutputDelta, ToolResult,
};

pub struct ToolRegistry {
    tools: BTreeMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: BTreeMap::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> Option<Box<dyn Tool>> {
        let name = tool.name().to_owned();
        self.tools.insert(name, tool)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for ToolRegistry {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    fn definitions_for_capabilities(
        &self,
        capabilities: &SessionInteractionCapabilities,
    ) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .filter(|tool| {
                !tool.requires_interactive_capability()
                    || capabilities.supports_interactive_components
                        && capabilities.can_use_question_tool()
            })
            .map(|tool| tool.definition())
            .collect()
    }

    fn tool_requires_runtime_context(&self, name: &str) -> bool {
        self.tools.get(name).map(|tool| tool.requires_runtime_context()).unwrap_or(false)
    }

    async fn call(
        &self,
        call: &ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        match self.tools.get(&call.tool_name) {
            Some(tool) => tool.call(call, output, context).await,
            None => Err(CoreError::new(format!("unknown tool: {}", call.tool_name))),
        }
    }
}
