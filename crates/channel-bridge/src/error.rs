#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelBridgeError {
    message: String,
}

impl ChannelBridgeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ChannelBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ChannelBridgeError {}
