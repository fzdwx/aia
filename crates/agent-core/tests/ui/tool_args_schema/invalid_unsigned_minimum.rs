use agent_core::ToolArgsSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ToolArgsSchema)]
struct InvalidUnsignedMinimumArgs {
    #[tool_schema(minimum = -1)]
    limit: u32,
}

fn main() {}
