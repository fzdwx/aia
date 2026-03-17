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
    use std::collections::BTreeSet;

    use agent_core::{Tool, ToolDefinition, ToolExecutor};

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
        assert_eq!(
            definition.description,
            "Execute a shell command with the embedded brush runtime"
        );
        assert_eq!(
            definition.parameters["properties"]["command"]["description"],
            "The shell command to execute"
        );
    }

    #[test]
    fn builtin_tool_definitions_match_schemars_output() {
        let shell = ShellTool.definition();
        assert_eq!(
            shell.parameters,
            ToolDefinition::new("shell", "Execute a shell command with the embedded brush runtime")
                .with_parameters_schema::<ShellToolArgs>()
                .parameters
        );

        let read = ReadTool.definition();
        assert_eq!(
            read.parameters,
            ToolDefinition::new("read", "Read a file with line numbers")
                .with_parameters_schema::<ReadToolArgs>()
                .parameters
        );

        let write = WriteTool.definition();
        assert_eq!(
            write.parameters,
            ToolDefinition::new("write", "Create or overwrite a file")
                .with_parameters_schema::<WriteToolArgs>()
                .parameters
        );

        let edit = EditTool.definition();
        assert_eq!(
            edit.parameters,
            ToolDefinition::new("edit", "Replace exact text in a file (must match uniquely)")
                .with_parameters_schema::<EditToolArgs>()
                .parameters
        );

        let apply_patch = ApplyPatchTool.definition();
        assert_eq!(
            apply_patch.parameters,
            ToolDefinition::new(
                "apply_patch",
                "Apply a patch in apply_patch format (supports Update File, Add File, Delete File, Move to)",
            )
            .with_parameters_schema::<ApplyPatchToolArgs>()
            .parameters
        );

        let glob = GlobTool.definition();
        assert_eq!(
            glob.parameters,
            ToolDefinition::new(
                "glob",
                "Find files matching a glob pattern (respects .gitignore and skips .git/node_modules/target)",
            )
            .with_parameters_schema::<GlobToolArgs>()
            .parameters
        );

        let grep = GrepTool.definition();
        assert_eq!(
            grep.parameters,
            ToolDefinition::new(
                "grep",
                "Search file contents with regex (respects .gitignore and skips .git/node_modules/target)",
            )
            .with_parameters_schema::<GrepToolArgs>()
            .parameters
        );
    }
}
