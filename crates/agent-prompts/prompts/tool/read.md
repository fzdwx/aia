Read a file or directory from the local filesystem. If the path does not exist, an error is returned.

Usage:
- The `file_path` parameter may be absolute or relative to the current workspace root.
- By default, this tool returns up to 2000 lines from the start of the file.
- The `offset` parameter is a 0-based starting line offset.
- To read later sections, call this tool again with a larger `offset`.
- Contents are returned with line numbers for text files.
- Use the `glob` tool when you need to discover filenames first, and use the `grep` tool when you need to search file contents by pattern.
