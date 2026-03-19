use agent_core_macros::ToolArgsSchema;
use serde::{Deserialize, Serialize};

const LABEL: &str = "App ID";

#[derive(Serialize, Deserialize, ToolArgsSchema)]
struct InvalidMetaValueArgs {
    #[tool_schema(meta(key = "x-label", value = LABEL))]
    query: String,
}

fn main() {}
