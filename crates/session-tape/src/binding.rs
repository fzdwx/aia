use agent_core::ModelRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionProviderBinding {
    Bootstrap,
    Provider {
        model_ref: ModelRef,
        #[serde(default)]
        reasoning_effort: Option<String>,
    },
}
