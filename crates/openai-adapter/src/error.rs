use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OpenAiAdapterError {
    message: String,
    status_code: Option<u16>,
    response_body: Option<String>,
    cancelled: bool,
}

impl OpenAiAdapterError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), status_code: None, response_body: None, cancelled: false }
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self { message: message.into(), status_code: None, response_body: None, cancelled: true }
    }

    pub fn with_status_code(mut self, status_code: Option<u16>) -> Self {
        self.status_code = status_code;
        self
    }

    pub fn with_response_body(mut self, response_body: Option<String>) -> Self {
        self.response_body = response_body;
        self
    }

    pub fn status_code(&self) -> Option<u16> {
        self.status_code
    }

    pub fn response_body(&self) -> Option<&str> {
        self.response_body.as_deref()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl fmt::Display for OpenAiAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for OpenAiAdapterError {}
