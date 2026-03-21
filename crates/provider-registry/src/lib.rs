mod error;
mod model;
mod registry;

#[cfg(test)]
#[path = "../tests/lib/mod.rs"]
mod tests;

pub use agent_core::ModelLimit;
pub use error::ProviderRegistryError;
pub use model::{ModelConfig, ProviderKind, ProviderProfile};
pub use registry::{ProviderRegistry, default_registry_path};
