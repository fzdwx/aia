use agent_core::{
    AbortSignal, QuestionAnswer, QuestionRequest, QuestionResult, QuestionResultStatus, Tool,
    ToolCall, ToolCallOutcome, ToolDefinition, ToolExecutionContext, ToolExecutor,
};
use agent_prompts::tool_descriptions::shell_tool_description;
use std::collections::BTreeSet;

use super::{
    ApplyPatchTool, CodeSearchTool, EditTool, GlobTool, GrepTool, QuestionTool, ReadTool,
    ShellTool, TapeHandoffTool, TapeInfoTool, WebSearchTool, WriteTool, build_tool_registry,
    question_tool_result_from_request,
};
use crate::apply_patch::ApplyPatchToolArgs;
use crate::codesearch::CodeSearchToolArgs;
use crate::edit::EditToolArgs;
use crate::glob::GlobToolArgs;
use crate::grep::GrepToolArgs;
use crate::question::QuestionToolArgs;
use crate::read::ReadToolArgs;
use crate::shell::ShellToolArgs;
use crate::tape::{TapeHandoffToolArgs, TapeInfoToolArgs};
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
        "CodeSearch",
        "WebSearch",
        "Question",
        "TapeInfo",
        "TapeHandoff",
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

    let question = QuestionTool.definition();
    assert_eq!(question.name, "Question");
    assert_eq!(
        question.parameters,
        ToolDefinition::new("Question", "ignored")
            .with_parameters_schema::<QuestionToolArgs>()
            .parameters
    );

    let tape_info = TapeInfoTool.definition();
    assert_eq!(
        tape_info.parameters,
        ToolDefinition::new("TapeInfo", "ignored")
            .with_parameters_schema::<TapeInfoToolArgs>()
            .parameters
    );

    let tape_handoff = TapeHandoffTool.definition();
    assert_eq!(
        tape_handoff.parameters,
        ToolDefinition::new("TapeHandoff", "ignored")
            .with_parameters_schema::<TapeHandoffToolArgs>()
            .parameters
    );
}

#[test]
fn builtin_registry_now_directly_exposes_question() {
    let registry = build_tool_registry();
    let names = registry
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();

    assert!(names.contains("Question"));
    assert!(names.contains("TapeInfo"));
    assert!(names.contains("TapeHandoff"));
}

#[tokio::test(flavor = "current_thread")]
async fn question_tool_returns_suspended_request_instead_of_completed_result() {
    let tool = QuestionTool;
    let call = ToolCall::new("Question").with_arguments_value(serde_json::json!({
        "questions": [{
            "id": "database",
            "question": "Use which database?",
            "kind": "choice",
            "required": true,
            "multi_select": false,
            "options": [{ "id": "sqlite", "label": "SQLite" }],
            "recommended_option_id": "sqlite",
            "recommendation_reason": "best local default"
        }]
    }));

    let outcome = tool
        .call(
            &call,
            &mut |_| {},
            &ToolExecutionContext {
                run_id: "turn-test".into(),
                session_id: Some("session-test".into()),
                workspace_root: None,
                abort: AbortSignal::new(),
                runtime: None,
            },
        )
        .await
        .expect("question tool should succeed");

    let ToolCallOutcome::Suspended { request } = outcome else {
        panic!("question tool should suspend instead of completing immediately");
    };

    assert_eq!(request.tool_name, "Question");
    assert_eq!(request.turn_id, "turn-test");
    assert_eq!(request.invocation_id, call.invocation_id);
    assert_eq!(request.kind, "question");

    let decoded: QuestionRequest =
        serde_json::from_value(request.payload).expect("payload should decode as question request");
    assert_eq!(decoded.turn_id, "turn-test");
    assert_eq!(decoded.invocation_id, call.invocation_id);
    assert_eq!(decoded.questions.len(), 1);
}

#[test]
fn question_tool_result_from_request_preserves_invocation_and_details() {
    let request = QuestionRequest {
        request_id: "qreq-1".into(),
        invocation_id: "call-1".into(),
        turn_id: "turn-1".into(),
        questions: vec![agent_core::QuestionItem {
            id: "database".into(),
            question: "Use which database?".into(),
            kind: agent_core::QuestionKind::Choice,
            required: true,
            multi_select: false,
            options: vec![agent_core::QuestionOption {
                id: "sqlite".into(),
                label: "SQLite".into(),
                description: None,
            }],
            placeholder: None,
            recommended_option_id: Some("sqlite".into()),
            recommendation_reason: Some("best local default".into()),
        }],
    };
    let result = QuestionResult {
        status: QuestionResultStatus::Answered,
        request_id: request.request_id.clone(),
        answers: vec![QuestionAnswer {
            question_id: "database".into(),
            selected_option_ids: vec!["sqlite".into()],
            text: None,
        }],
        reason: None,
    };

    let tool_result =
        question_tool_result_from_request(&request, &result).expect("tool result should build");

    assert_eq!(tool_result.tool_name, "Question");
    assert_eq!(tool_result.invocation_id, request.invocation_id);
    assert!(tool_result.content.contains("answered"));
    assert_eq!(
        tool_result
            .details
            .as_ref()
            .and_then(|details: &serde_json::Value| details.get("request_id")),
        Some(&serde_json::json!("qreq-1"))
    );
}

#[test]
fn question_tool_result_from_request_rejects_unknown_question_ids() {
    let request = QuestionRequest {
        request_id: "qreq-1".into(),
        invocation_id: "call-1".into(),
        turn_id: "turn-1".into(),
        questions: vec![agent_core::QuestionItem {
            id: "database".into(),
            question: "Use which database?".into(),
            kind: agent_core::QuestionKind::Choice,
            required: true,
            multi_select: false,
            options: vec![agent_core::QuestionOption {
                id: "sqlite".into(),
                label: "SQLite".into(),
                description: None,
            }],
            placeholder: None,
            recommended_option_id: None,
            recommendation_reason: None,
        }],
    };
    let result = QuestionResult {
        status: QuestionResultStatus::Answered,
        request_id: request.request_id.clone(),
        answers: vec![QuestionAnswer {
            question_id: "language".into(),
            selected_option_ids: vec!["rust".into()],
            text: None,
        }],
        reason: None,
    };

    let error = question_tool_result_from_request(&request, &result)
        .expect_err("unknown answers should be rejected");

    assert!(error.to_string().contains("language"));
}
