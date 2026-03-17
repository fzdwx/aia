Executes a shell command in the embedded brush runtime.

Usage:
- Runs from the current workspace root by default.
- Streams stdout and stderr while the command runs, then returns the final exit code in the result details.
- Use this tool for terminal operations such as invoking git, package managers, test runners, or other CLI programs.
- Do not use this tool for file reads, writes, or searches when a dedicated tool already exists.
