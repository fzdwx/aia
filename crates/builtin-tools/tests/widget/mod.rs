use std::{future::Future, path::Path};

use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext, ToolOutputStream};

use super::{WidgetReadmeTool, WidgetRendererTool};

fn run_async<T>(future: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build");
    runtime.block_on(future)
}

fn test_context() -> ToolExecutionContext {
    ToolExecutionContext {
        run_id: "test-run".into(),
        session_id: None,
        workspace_root: Some(Path::new(".").to_path_buf()),
        abort: AbortSignal::new(),
        runtime: None,
        runtime_host: None,
    }
}

#[test]
fn widget_readme_returns_base_guidelines_when_modules_are_omitted() {
    let tool = WidgetReadmeTool;
    let call = ToolCall::new("WidgetReadme").with_arguments_value(serde_json::json!({}));

    let result = run_async(tool.call(&call, &mut |_| {}, &test_context()))
        .expect("widget readme should return a result");

    assert!(result.content.contains("# Widget renderer — Visual Creation Suite"));
    assert!(result.content.contains("## Modules"));
    assert!(result.content.contains("## Core Design System"));
    assert!(result.content.contains("## When nothing fits"));
    assert!(!result.content.contains("## SVG setup"));

    let details = result.details.expect("widget readme should include details");
    assert_eq!(details["modules"], serde_json::json!([]));
}

#[test]
fn widget_readme_assembles_diagram_sections_from_full_template() {
    let tool = WidgetReadmeTool;
    let call = ToolCall::new("WidgetReadme").with_arguments_value(serde_json::json!({
        "modules": ["diagram"],
    }));

    let result = run_async(tool.call(&call, &mut |_| {}, &test_context()))
        .expect("widget readme should return diagram guidance");

    assert!(result.content.contains("## SVG setup"));
    assert!(result.content.contains("## Color palette"));
    assert!(result.content.contains("## Diagram types"));
    assert!(!result.content.contains("## Charts (Chart.js)"));
}

#[test]
fn widget_readme_assembles_interactive_sections_from_full_template() {
    let tool = WidgetReadmeTool;
    let call = ToolCall::new("WidgetReadme").with_arguments_value(serde_json::json!({
        "modules": ["interactive"],
    }));

    let result = run_async(tool.call(&call, &mut |_| {}, &test_context()))
        .expect("widget readme should return interactive guidance");

    assert!(result.content.contains("## UI components"));
    assert!(result.content.contains("### 1. Interactive explainer — learn how something works"));
    assert!(!result.content.contains("### 2. Compare options — decision making"));
    assert!(!result.content.contains("## Diagram types"));
}

#[test]
fn widget_renderer_emits_html_as_stream_delta_and_details() {
    let tool = WidgetRendererTool;
    let call = ToolCall::new("WidgetRenderer").with_arguments_value(serde_json::json!({
        "title": "Live widget",
        "description": "Streaming preview",
        "html": "<div class=\"card\">hello</div>",
    }));
    let mut deltas = Vec::new();

    let result = run_async(tool.call(&call, &mut |delta| deltas.push(delta), &test_context()))
        .expect("widget renderer should return a result");

    let stdout = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
        .map(|delta| delta.text.as_str())
        .collect::<String>();
    assert_eq!(stdout, "<div class=\"card\">hello</div>");

    let details = result.details.expect("widget renderer should include details");
    assert_eq!(details["title"], "Live widget");
    assert_eq!(details["description"], "Streaming preview");
    assert_eq!(details["html"], "<div class=\"card\">hello</div>");
}

#[test]
fn widget_renderer_falls_back_to_description_when_title_is_missing() {
    let tool = WidgetRendererTool;
    let call = ToolCall::new("WidgetRenderer").with_arguments_value(serde_json::json!({
        "description": "Interactive particle sandbox for momentum transfer.",
        "html": "<div class=\"card\">hello</div>",
    }));

    let result = run_async(tool.call(&call, &mut |_| {}, &test_context()))
        .expect("widget renderer should derive a fallback title");

    let details = result.details.expect("widget renderer should include details");
    assert_eq!(details["title"], "Interactive particle sandbox for momentum transf");
}

#[test]
fn widget_renderer_falls_back_when_title_is_blank() {
    let tool = WidgetRendererTool;
    let call = ToolCall::new("WidgetRenderer").with_arguments_value(serde_json::json!({
        "title": "   ",
        "description": "Flowchart visualizer",
        "html": "<div class=\"card\">hello</div>",
    }));

    let result = run_async(tool.call(&call, &mut |_| {}, &test_context()))
        .expect("blank title should not fail widget renderer");

    let details = result.details.expect("widget renderer should include details");
    assert_eq!(details["title"], "Flowchart visualizer");
}
