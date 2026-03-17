Fast file pattern matching tool that works across the workspace.

Usage:
- Supports glob patterns like `**/*.rs` or `src/**/*.ts`.
- Returns matching file paths sorted by modification time.
- Respects `.gitignore` and skips common large directories such as `.git`, `node_modules`, and `target`.
- Use this tool when you need to find files by name patterns.
