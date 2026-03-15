use std::collections::BTreeMap;

use crate::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolExecutor, ToolOutputDelta,
    ToolResult,
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

impl ToolExecutor for ToolRegistry {
    type Error = CoreError;

    fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    fn call(
        &self,
        call: &ToolCall,
        output: &mut dyn FnMut(ToolOutputDelta),
        context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        match self.tools.get(&call.tool_name) {
            Some(tool) => tool.call(call, output, context),
            None => Err(CoreError::new(format!("unknown tool: {}", call.tool_name))),
        }
    }
}
