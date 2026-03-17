mod apply_patch;
mod edit;
mod glob;
mod grep;
mod read;
mod shell;
mod walk;
mod write;

pub use apply_patch::ApplyPatchTool;
pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use shell::ShellTool;
pub use write::WriteTool;

pub fn should_skip_directory_name(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target")
}

use agent_core::ToolRegistry;

pub fn build_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ShellTool));
    registry.register(Box::new(ReadTool));
    registry.register(Box::new(WriteTool));
    registry.register(Box::new(EditTool));
    registry.register(Box::new(ApplyPatchTool));
    registry.register(Box::new(GlobTool));
    registry.register(Box::new(GrepTool));
    registry
}

#[cfg(test)]
mod tests {
    use agent_core::{Tool, ToolExecutor};
    use agent_prompts::tool_descriptions::shell_tool_description;
    use std::collections::BTreeSet;

    use super::{
        ApplyPatchTool, EditTool, GlobTool, GrepTool, ReadTool, ShellTool, WriteTool,
        build_tool_registry,
    };
    use crate::edit::edit_tool_parameters;
    use crate::glob::glob_tool_parameters;
    use crate::grep::grep_tool_parameters;
    use crate::read::read_tool_parameters;
    use crate::shell::shell_tool_parameters;
    use crate::write::write_tool_parameters;

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
    fn builtin_tool_definitions_match_raw_json_output() {
        let shell = ShellTool.definition();
        assert_eq!(shell.parameters, shell_tool_parameters());

        let read = ReadTool.definition();
        assert_eq!(read.parameters, read_tool_parameters());
        assert!(read.parameters.get("title").is_none());
        assert_eq!(read.parameters["properties"]["offset"]["type"], "integer");
        assert_eq!(read.parameters["properties"]["limit"]["type"], "integer");

        let write = WriteTool.definition();
        assert_eq!(write.parameters, write_tool_parameters());

        let edit = EditTool.definition();
        assert_eq!(edit.parameters, edit_tool_parameters());

        let apply_patch = ApplyPatchTool.definition();
        assert_eq!(apply_patch.parameters["type"], "object");
        assert!(apply_patch.parameters.get("$defs").is_none());
        assert!(apply_patch.parameters.get("anyOf").is_none());
        assert_eq!(apply_patch.parameters["properties"]["patch"]["type"], "string");
        assert_eq!(apply_patch.parameters["properties"]["patchText"]["type"], "string");
        assert_eq!(apply_patch.parameters["additionalProperties"], false);

        let glob = GlobTool.definition();
        assert_eq!(glob.parameters, glob_tool_parameters());
        assert!(glob.parameters.get("title").is_none());
        assert_eq!(glob.parameters["properties"]["path"]["type"], "string");
        assert_eq!(glob.parameters["properties"]["limit"]["type"], "integer");

        let grep = GrepTool.definition();
        assert_eq!(grep.parameters, grep_tool_parameters());
        assert!(grep.parameters.get("title").is_none());
        assert_eq!(grep.parameters["properties"]["path"]["type"], "string");
        assert_eq!(grep.parameters["properties"]["glob"]["type"], "string");
        assert_eq!(grep.parameters["properties"]["limit"]["type"], "integer");
    }
}
