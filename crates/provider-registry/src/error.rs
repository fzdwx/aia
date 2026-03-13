use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProviderRegistryError {
    message: String,
}

impl ProviderRegistryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for ProviderRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProviderRegistryError {}
