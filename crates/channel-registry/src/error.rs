#[derive(Debug, Clone)]
pub struct ChannelRegistryError {
    message: String,
}

impl ChannelRegistryError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl std::fmt::Display for ChannelRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ChannelRegistryError {}
