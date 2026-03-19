use agent_core_macros::ToolArgsSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ToolArgsSchema)]
#[tool_schema(description = "bad")]
struct InvalidContainerKeyArgs {
    query: String,
}

fn main() {}
