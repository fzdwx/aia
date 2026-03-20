mod error;
mod model;
mod registry;

#[cfg(test)]
#[path = "../tests/lib/mod.rs"]
mod tests;

pub use error::ProviderRegistryError;
pub use model::{ModelConfig, ModelLimit, ProviderKind, ProviderProfile};
pub use registry::{ProviderRegistry, default_registry_path};
