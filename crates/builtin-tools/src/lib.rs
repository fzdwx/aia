mod edit;
mod glob;
mod grep;
mod read;
mod shell;
mod write;

pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use shell::ShellTool;
pub use write::WriteTool;

use agent_core::ToolRegistry;

pub fn build_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ShellTool));
    registry.register(Box::new(ReadTool));
    registry.register(Box::new(WriteTool));
    registry.register(Box::new(EditTool));
    registry.register(Box::new(GlobTool));
    registry.register(Box::new(GrepTool));
    registry
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agent_core::{Tool, ToolExecutor};

    use super::{ShellTool, build_tool_registry};

    #[test]
    fn registry_exposes_only_new_tool_names() {
        let registry = build_tool_registry();
        let names = registry
            .definitions()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();

        let expected = ["shell", "read", "write", "edit", "glob", "grep"]
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
}
