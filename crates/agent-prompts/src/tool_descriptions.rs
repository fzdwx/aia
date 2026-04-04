macro_rules! markdown_tool_description {
    ($fn_name:ident, $path:literal) => {
        pub fn $fn_name() -> &'static str {
            include_str!($path).trim()
        }
    };
}

markdown_tool_description!(shell_tool_description, "../prompts/tool/shell.md");
markdown_tool_description!(read_tool_description, "../prompts/tool/read.md");
markdown_tool_description!(write_tool_description, "../prompts/tool/write.md");
markdown_tool_description!(edit_tool_description, "../prompts/tool/edit.md");
markdown_tool_description!(apply_patch_tool_description, "../prompts/tool/apply_patch.md");
markdown_tool_description!(glob_tool_description, "../prompts/tool/glob.md");
markdown_tool_description!(grep_tool_description, "../prompts/tool/grep.md");
markdown_tool_description!(codesearch_tool_description, "../prompts/tool/codesearch.md");
markdown_tool_description!(websearch_tool_description, "../prompts/tool/websearch.md");
markdown_tool_description!(question_tool_description, "../prompts/tool/question.md");
markdown_tool_description!(tape_info_tool_description, "../prompts/tool/tape_info.md");
markdown_tool_description!(tape_handoff_tool_description, "../prompts/tool/tape_handoff.md");
markdown_tool_description!(widget_readme_tool_description, "../prompts/tool/widget_readme.md");
markdown_tool_description!(widget_renderer_tool_description, "../prompts/tool/widget_renderer.md");

#[cfg(test)]
#[path = "../tests/tool_descriptions/mod.rs"]
mod tests;
