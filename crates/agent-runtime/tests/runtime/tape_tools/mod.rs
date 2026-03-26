use super::{
    TapeHandoffTool, TapeHandoffToolArgs, TapeInfoTool, TapeInfoToolArgs, runtime_tool_definitions,
    runtime_tool_definitions_for,
};
use crate::runtime::tape_tools::Tool;
use agent_core::{SessionInteractionCapabilities, ToolDefinition};

#[test]
fn runtime_tool_definitions_match_derive_schema_output() {
    let definitions = runtime_tool_definitions();
    assert_eq!(definitions.len(), 2);

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
}

#[test]
fn runtime_tool_definitions_ignore_interaction_capabilities() {
    let definitions =
        runtime_tool_definitions_for(&SessionInteractionCapabilities::non_interactive());

    assert_eq!(definitions.len(), 2);
}
