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
markdown_tool_description!(tape_info_tool_description, "../prompts/tool/tape_info.md");
markdown_tool_description!(tape_handoff_tool_description, "../prompts/tool/tape_handoff.md");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_descriptions_are_loaded_from_markdown() {
        let cases: [(&str, &str); 9] = [
            (shell_tool_description(), include_str!("../prompts/tool/shell.md").trim()),
            (read_tool_description(), include_str!("../prompts/tool/read.md").trim()),
            (write_tool_description(), include_str!("../prompts/tool/write.md").trim()),
            (edit_tool_description(), include_str!("../prompts/tool/edit.md").trim()),
            (apply_patch_tool_description(), include_str!("../prompts/tool/apply_patch.md").trim()),
            (glob_tool_description(), include_str!("../prompts/tool/glob.md").trim()),
            (grep_tool_description(), include_str!("../prompts/tool/grep.md").trim()),
            (tape_info_tool_description(), include_str!("../prompts/tool/tape_info.md").trim()),
            (
                tape_handoff_tool_description(),
                include_str!("../prompts/tool/tape_handoff.md").trim(),
            ),
        ];

        for (loaded, expected) in cases {
            assert_eq!(loaded, expected);
            assert!(!loaded.is_empty());
        }
    }
}
