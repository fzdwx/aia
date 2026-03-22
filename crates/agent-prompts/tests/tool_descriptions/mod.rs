use super::*;

#[test]
fn tool_descriptions_are_loaded_from_markdown() {
    let cases: [(&str, &str); 11] = [
        (shell_tool_description(), include_str!("../../prompts/tool/shell.md").trim()),
        (read_tool_description(), include_str!("../../prompts/tool/read.md").trim()),
        (write_tool_description(), include_str!("../../prompts/tool/write.md").trim()),
        (edit_tool_description(), include_str!("../../prompts/tool/edit.md").trim()),
        (apply_patch_tool_description(), include_str!("../../prompts/tool/apply_patch.md").trim()),
        (glob_tool_description(), include_str!("../../prompts/tool/glob.md").trim()),
        (grep_tool_description(), include_str!("../../prompts/tool/grep.md").trim()),
        (codesearch_tool_description(), include_str!("../../prompts/tool/codesearch.md").trim()),
        (websearch_tool_description(), include_str!("../../prompts/tool/websearch.md").trim()),
        (tape_info_tool_description(), include_str!("../../prompts/tool/tape_info.md").trim()),
        (
            tape_handoff_tool_description(),
            include_str!("../../prompts/tool/tape_handoff.md").trim(),
        ),
    ];

    for (loaded, expected) in cases {
        assert_eq!(loaded, expected);
        assert!(!loaded.is_empty());
    }
}
