use agent_core::{
    CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolOutputStream, ToolResult,
};
use agent_core_macros::ToolArgsSchema as DeriveToolArgsSchema;
use agent_prompts::{
    tool_descriptions::{widget_readme_tool_description, widget_renderer_tool_description},
    widget_guidelines::{WIDGET_GUIDELINE_MODULES, widget_guideline_document_for_modules},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct WidgetReadmeTool;
pub struct WidgetRendererTool;

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WidgetReadmeToolArgs {
    #[tool_schema(
        description = "Optional widget guidance modules to load. Choose from art, mockup, interactive, chart, diagram."
    )]
    modules: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, DeriveToolArgsSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WidgetRendererToolArgs {
    #[tool_schema(description = "Short title for the widget.")]
    title: String,
    #[tool_schema(description = "One-sentence explanation of what the widget demonstrates.")]
    description: String,
    #[tool_schema(
        description = "Self-contained HTML or SVG fragment to render in the widget sandbox."
    )]
    html: String,
}

fn normalize_widget_modules(raw_modules: Option<Vec<String>>) -> Result<Vec<String>, CoreError> {
    let mut normalized = Vec::new();

    for module in raw_modules.unwrap_or_default() {
        let module_name = module.trim().to_ascii_lowercase();
        if module_name.is_empty() || normalized.iter().any(|existing| existing == &module_name) {
            continue;
        }
        if !WIDGET_GUIDELINE_MODULES.contains(&module_name.as_str()) {
            return Err(CoreError::new(format!(
                "invalid widget module: {module_name}; expected one of {}",
                WIDGET_GUIDELINE_MODULES.join(", ")
            )));
        }
        normalized.push(module_name);
    }

    Ok(normalized)
}

fn emit_html_stream(html: &str, output: &mut (dyn FnMut(ToolOutputDelta) + Send)) {
    const MAX_CHUNK_BYTES: usize = 4096;

    if html.is_empty() {
        return;
    }

    let mut start = 0;
    while start < html.len() {
        let mut end = (start + MAX_CHUNK_BYTES).min(html.len());
        while end < html.len() && !html.is_char_boundary(end) {
            end -= 1;
        }
        if end <= start {
            end = html.len();
        }

        output(ToolOutputDelta {
            stream: ToolOutputStream::Stdout,
            text: html[start..end].to_string(),
        });
        start = end;
    }
}

#[async_trait]
impl Tool for WidgetReadmeTool {
    fn name(&self) -> &str {
        "WidgetReadme"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), widget_readme_tool_description())
            .with_parameters_schema::<WidgetReadmeToolArgs>()
    }

    fn requires_interactive_capability(&self) -> bool {
        true
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        _context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: WidgetReadmeToolArgs = tool_call.parse_arguments()?;
        let modules = normalize_widget_modules(args.modules)?;
        let content = widget_guideline_document_for_modules(&modules);
        let details = json!({
            "modules": modules,
            "available_modules": WIDGET_GUIDELINE_MODULES,
            "loaded_detailed_guidance": !content.trim().is_empty(),
        });

        Ok(ToolResult::from_call(tool_call, content).with_details(details))
    }
}

#[async_trait]
impl Tool for WidgetRendererTool {
    fn name(&self) -> &str {
        "WidgetRenderer"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), widget_renderer_tool_description())
            .with_parameters_schema::<WidgetRendererToolArgs>()
    }

    fn requires_interactive_capability(&self) -> bool {
        true
    }

    async fn call(
        &self,
        tool_call: &ToolCall,
        output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        _context: &ToolExecutionContext,
    ) -> Result<ToolResult, CoreError> {
        let args: WidgetRendererToolArgs = tool_call.parse_arguments()?;

        let title = args.title.trim();
        let description = args.description.trim();
        let html = args.html.trim();

        if title.is_empty() {
            return Err(CoreError::new("widget title must not be empty"));
        }
        if description.is_empty() {
            return Err(CoreError::new("widget description must not be empty"));
        }
        if html.is_empty() {
            return Err(CoreError::new("widget html must not be empty"));
        }

        emit_html_stream(html, output);

        let details = json!({
            "title": title,
            "description": description,
            "html": html,
            "content_type": "text/html",
        });

        Ok(ToolResult::from_call(tool_call, format!("Rendered widget: {title}"))
            .with_details(details))
    }
}

#[cfg(test)]
#[path = "../tests/widget/mod.rs"]
mod tests;
