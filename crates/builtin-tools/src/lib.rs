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
#[path = "../tests/lib/mod.rs"]
mod tests;
