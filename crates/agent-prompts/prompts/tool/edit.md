Performs exact string replacements in files.

Usage:
- `old_string` must match exactly once in the file.
- The edit fails when `old_string` is not found.
- The edit also fails when `old_string` matches multiple times; in that case, provide more surrounding context to make the match unique.
- Use this tool to make precise, minimal changes to an existing file.
