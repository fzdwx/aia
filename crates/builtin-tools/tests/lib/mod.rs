use agent_core::{Tool, ToolDefinition, ToolExecutor};
use agent_prompts::tool_descriptions::shell_tool_description;
use std::collections::BTreeSet;

use super::{
    ApplyPatchTool, EditTool, GlobTool, GrepTool, ReadTool, ShellTool, WriteTool,
    build_tool_registry,
};
use crate::apply_patch::ApplyPatchToolArgs;
use crate::edit::EditToolArgs;
use crate::glob::GlobToolArgs;
use crate::grep::GrepToolArgs;
use crate::read::ReadToolArgs;
use crate::shell::ShellToolArgs;
use crate::write::WriteToolArgs;

#[test]
fn registry_exposes_only_new_tool_names() {
    let registry = build_tool_registry();
    let names = registry
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();

    let expected = ["shell", "read", "write", "edit", "apply_patch", "glob", "grep"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    assert_eq!(names, expected);
    assert!(!names.contains("bash"));
    assert!(!names.contains("read_file"));
    assert!(!names.contains("write_file"));
    assert!(!names.contains("edit_file"));
}

#[test]
fn shell_tool_definition_uses_shell_name_and_brush_runtime() {
    let definition = ShellTool.definition();

    assert_eq!(definition.name, "shell");
    assert_eq!(definition.description, shell_tool_description());
    assert_eq!(
        definition.parameters["properties"]["command"]["description"],
        "The shell command to execute"
    );
}

#[test]
fn builtin_tool_definitions_match_derive_schema_output() {
    let shell = ShellTool.definition();
    assert_eq!(
        shell.parameters,
        ToolDefinition::new("shell", "ignored")
            .with_parameters_schema::<ShellToolArgs>()
            .parameters
    );

    let read = ReadTool.definition();
    assert_eq!(
        read.parameters,
        ToolDefinition::new("read", "ignored").with_parameters_schema::<ReadToolArgs>().parameters
    );
    assert!(read.parameters.get("title").is_none());
    assert_eq!(read.parameters["properties"]["offset"]["type"], "integer");
    assert_eq!(read.parameters["properties"]["limit"]["type"], "integer");

    let write = WriteTool.definition();
    assert_eq!(
        write.parameters,
        ToolDefinition::new("write", "ignored")
            .with_parameters_schema::<WriteToolArgs>()
            .parameters
    );

    let edit = EditTool.definition();
    assert_eq!(
        edit.parameters,
        ToolDefinition::new("edit", "ignored").with_parameters_schema::<EditToolArgs>().parameters
    );

    let apply_patch = ApplyPatchTool.definition();
    assert_eq!(
        apply_patch.parameters,
        ToolDefinition::new("apply_patch", "ignored")
            .with_parameters_schema::<ApplyPatchToolArgs>()
            .parameters
    );

    let glob = GlobTool.definition();
    assert_eq!(
        glob.parameters,
        ToolDefinition::new("glob", "ignored").with_parameters_schema::<GlobToolArgs>().parameters
    );
    assert!(glob.parameters.get("title").is_none());
    assert_eq!(glob.parameters["properties"]["path"]["type"], "string");
    assert_eq!(glob.parameters["properties"]["limit"]["type"], "integer");

    let grep = GrepTool.definition();
    assert_eq!(
        grep.parameters,
        ToolDefinition::new("grep", "ignored").with_parameters_schema::<GrepToolArgs>().parameters
    );
    assert!(grep.parameters.get("title").is_none());
    assert_eq!(grep.parameters["properties"]["path"]["type"], "string");
    assert_eq!(grep.parameters["properties"]["glob"]["type"], "string");
    assert_eq!(grep.parameters["properties"]["limit"]["type"], "integer");
}
