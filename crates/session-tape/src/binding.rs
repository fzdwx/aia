use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionProviderBinding {
    Bootstrap,
    Provider {
        name: String,
        model: String,
        base_url: String,
        #[serde(default = "default_provider_protocol")]
        protocol: String,
    },
}

pub(crate) fn default_provider_protocol() -> String {
    "openai-responses".into()
}
