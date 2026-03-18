use agent_core::ToolArgsSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ToolArgsSchema)]
struct InvalidFieldKeyArgs {
    #[tool_schema(title = "bad")]
    query: String,
}

fn main() {}
