use super::{
    TapeHandoffTool, TapeHandoffToolArgs, TapeInfoTool, TapeInfoToolArgs, runtime_tool_definitions,
    runtime_tool_definitions_for,
};
use crate::runtime::tape_tools::Tool;
use agent_core::{SessionInteractionCapabilities, ToolDefinition};

#[test]
fn runtime_tool_definitions_match_derive_schema_output() {
    let definitions = runtime_tool_definitions();
    assert_eq!(definitions.len(), 3);

    let tape_info = TapeInfoTool.definition();
    assert!(definitions.iter().any(|definition| definition == &tape_info));
    assert_eq!(
        tape_info.parameters,
        ToolDefinition::new("TapeInfo", "ignored")
            .with_parameters_schema::<TapeInfoToolArgs>()
            .parameters
    );
    assert_eq!(tape_info.name, "TapeInfo");

    let tape_handoff = TapeHandoffTool.definition();
    assert!(definitions.iter().any(|definition| definition == &tape_handoff));
    assert_eq!(
        tape_handoff.parameters,
        ToolDefinition::new("TapeHandoff", "ignored")
            .with_parameters_schema::<TapeHandoffToolArgs>()
            .parameters
    );
    assert_eq!(tape_handoff.name, "TapeHandoff");

    assert!(definitions.iter().any(|definition| definition.name == "Question"));
}

#[test]
fn runtime_tool_definitions_only_expose_question_for_interactive_sessions() {
    let interactive = runtime_tool_definitions_for(&SessionInteractionCapabilities::interactive());
    let non_interactive =
        runtime_tool_definitions_for(&SessionInteractionCapabilities::non_interactive());

    assert_eq!(interactive.len(), 3);
    assert!(interactive.iter().any(|definition| definition.name == "Question"));

    assert_eq!(non_interactive.len(), 2);
    assert!(!non_interactive.iter().any(|definition| definition.name == "Question"));
}
