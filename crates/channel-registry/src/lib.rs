mod error;
mod model;
mod registry;

#[cfg(test)]
mod tests;

pub use error::ChannelRegistryError;
pub use model::{ChannelProfile, ChannelTransport};
pub use registry::{ChannelRegistry, default_registry_path};
