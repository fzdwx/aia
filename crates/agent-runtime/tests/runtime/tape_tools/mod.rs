use super::{runtime_tool_definitions, runtime_tool_definitions_for};
use agent_core::{SessionInteractionCapabilities, Tool};
use builtin_tools::{QuestionTool, TapeHandoffTool, TapeInfoTool};

#[test]
fn runtime_tool_definitions_match_derive_schema_output() {
    let definitions = runtime_tool_definitions();
    assert_eq!(definitions.len(), 3);

    let tape_info = TapeInfoTool.definition();
    assert!(definitions.iter().any(|definition| definition == &tape_info));
    assert_eq!(tape_info.name, "TapeInfo");

    let tape_handoff = TapeHandoffTool.definition();
    assert!(definitions.iter().any(|definition| definition == &tape_handoff));
    assert_eq!(tape_handoff.name, "TapeHandoff");

    let question = QuestionTool.definition();
    assert!(definitions.iter().any(|definition| definition == &question));
    assert_eq!(question.name, "Question");
}

#[test]
fn runtime_tool_definitions_ignore_interaction_capabilities() {
    let definitions =
        runtime_tool_definitions_for(&SessionInteractionCapabilities::non_interactive());

    assert_eq!(definitions.len(), 2);
}
