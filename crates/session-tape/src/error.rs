use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionTapeError {
    message: String,
}

impl SessionTapeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub(crate) fn from_io(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }

    pub(crate) fn from_serde(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl fmt::Display for SessionTapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SessionTapeError {}
