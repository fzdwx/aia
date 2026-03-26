use agent_core::{Tool, ToolDefinition, ToolExecutor};
use agent_prompts::tool_descriptions::shell_tool_description;
use std::collections::BTreeSet;

use super::{
    ApplyPatchTool, CodeSearchTool, EditTool, GlobTool, GrepTool, QuestionTool, ReadTool,
    ShellTool, TapeHandoffTool, TapeInfoTool, WebSearchTool, WriteTool, build_tool_registry,
};
use crate::apply_patch::ApplyPatchToolArgs;
use crate::codesearch::CodeSearchToolArgs;
use crate::edit::EditToolArgs;
use crate::glob::GlobToolArgs;
use crate::grep::GrepToolArgs;
use crate::question::QuestionToolArgs;
use crate::read::ReadToolArgs;
use crate::shell::ShellToolArgs;
use crate::websearch::WebSearchToolArgs;
use crate::write::WriteToolArgs;

#[test]
fn registry_exposes_only_new_tool_names() {
    let registry = build_tool_registry();
    let names = registry
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();

    let expected = [
        "Shell",
        "Read",
        "Write",
        "Edit",
        "ApplyPatch",
        "Glob",
        "Grep",
        "Question",
        "TapeInfo",
        "TapeHandoff",
        "CodeSearch",
        "WebSearch",
    ]
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

    assert_eq!(definition.name, "Shell");
    assert_eq!(definition.description, shell_tool_description());
    assert_eq!(
        definition.parameters["properties"]["command"]["description"],
        "The shell command to execute"
    );
    assert_eq!(
        definition.parameters["properties"]["description"]["description"],
        "Clear, concise description of what this command does in 5-10 words. Examples:\nInput: ls\nOutput: Lists files in current directory\n\nInput: git status\nOutput: Shows working tree status\n\nInput: npm install\nOutput: Installs package dependencies\n\nInput: mkdir foo\nOutput: Creates directory 'foo'"
    );
}

#[test]
fn builtin_tool_definitions_match_derive_schema_output() {
    let shell = ShellTool.definition();
    assert_eq!(
        shell.parameters,
        ToolDefinition::new("Shell", "ignored")
            .with_parameters_schema::<ShellToolArgs>()
            .parameters
    );

    let read = ReadTool.definition();
    assert_eq!(
        read.parameters,
        ToolDefinition::new("Read", "ignored").with_parameters_schema::<ReadToolArgs>().parameters
    );
    assert!(read.parameters.get("title").is_none());
    assert_eq!(read.parameters["properties"]["offset"]["type"], "integer");
    assert_eq!(read.parameters["properties"]["limit"]["type"], "integer");

    let write = WriteTool.definition();
    assert_eq!(
        write.parameters,
        ToolDefinition::new("Write", "ignored")
            .with_parameters_schema::<WriteToolArgs>()
            .parameters
    );

    let edit = EditTool.definition();
    assert_eq!(
        edit.parameters,
        ToolDefinition::new("Edit", "ignored").with_parameters_schema::<EditToolArgs>().parameters
    );

    let apply_patch = ApplyPatchTool.definition();
    assert_eq!(
        apply_patch.parameters,
        ToolDefinition::new("ApplyPatch", "ignored")
            .with_parameters_schema::<ApplyPatchToolArgs>()
            .parameters
    );

    let glob = GlobTool.definition();
    assert_eq!(
        glob.parameters,
        ToolDefinition::new("Glob", "ignored").with_parameters_schema::<GlobToolArgs>().parameters
    );
    assert!(glob.parameters.get("title").is_none());
    assert_eq!(glob.parameters["properties"]["path"]["type"], "string");
    assert_eq!(glob.parameters["properties"]["limit"]["type"], "integer");

    let grep = GrepTool.definition();
    assert_eq!(
        grep.parameters,
        ToolDefinition::new("Grep", "ignored").with_parameters_schema::<GrepToolArgs>().parameters
    );
    assert!(grep.parameters.get("title").is_none());
    assert_eq!(grep.parameters["properties"]["path"]["type"], "string");
    assert_eq!(grep.parameters["properties"]["glob"]["type"], "string");
    assert_eq!(grep.parameters["properties"]["limit"]["type"], "integer");

    let question = QuestionTool.definition();
    assert_eq!(
        question.parameters,
        ToolDefinition::new("Question", "ignored")
            .with_parameters_schema::<QuestionToolArgs>()
            .parameters
    );

    let tape_info = TapeInfoTool.definition();
    assert_eq!(tape_info.name, "TapeInfo");

    let tape_handoff = TapeHandoffTool.definition();
    assert_eq!(tape_handoff.name, "TapeHandoff");

    let codesearch = CodeSearchTool.definition();
    assert_eq!(codesearch.name, "CodeSearch");
    assert_eq!(codesearch.parameters["type"], "object");
    assert_eq!(codesearch.parameters["additionalProperties"], false);
    assert_eq!(codesearch.parameters["properties"]["query"]["type"], "string");
    assert_eq!(codesearch.parameters["properties"]["tokensNum"]["type"], "integer");
    assert_eq!(codesearch.parameters["properties"]["tokensNum"]["minimum"], 1000);
    assert_eq!(codesearch.parameters["properties"]["tokensNum"]["maximum"], 50000);
    assert_eq!(codesearch.parameters["properties"]["tokensNum"]["default"], 5000);

    let parsed: CodeSearchToolArgs = serde_json::from_value(serde_json::json!({
        "query": "React useState hook examples",
        "tokensNum": 6000,
    }))
    .expect("codesearch args should deserialize");
    assert_eq!(parsed.tokens_num, 6000);

    let websearch = WebSearchTool.definition();
    assert_eq!(websearch.name, "WebSearch");
    assert!(websearch.description.contains("2026"));
    assert_eq!(websearch.parameters["properties"]["numResults"]["default"], 8);
    assert_eq!(websearch.parameters["properties"]["livecrawl"]["type"], "string");
    assert_eq!(websearch.parameters["properties"]["type"]["type"], "string");

    let parsed: WebSearchToolArgs = serde_json::from_value(serde_json::json!({
        "query": "AI news 2026",
        "numResults": 5,
        "livecrawl": "preferred",
        "type": "deep",
        "contextMaxCharacters": 8000,
    }))
    .expect("websearch args should deserialize");
    assert_eq!(parsed.num_results, 5);
    assert_eq!(parsed.context_max_characters, Some(8000));
}
