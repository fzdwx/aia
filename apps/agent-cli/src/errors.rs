use std::{error::Error, io};

use agent_core::CoreError;
use agent_runtime::RuntimeError;
use openai_adapter::OpenAiAdapterError;
use provider_registry::ProviderRegistryError;
use session_tape::SessionTapeError;

#[derive(Debug)]
pub enum CliModelError {
    Bootstrap(CoreError),
    OpenAi(OpenAiAdapterError),
}

impl std::fmt::Display for CliModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bootstrap(error) => write!(f, "{error}"),
            Self::OpenAi(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CliModelError {}

#[derive(Debug)]
pub enum CliSetupError {
    Io(io::Error),
    Registry(ProviderRegistryError),
    OpenAiAdapter(OpenAiAdapterError),
    Message(String),
}

impl std::fmt::Display for CliSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Registry(error) => write!(f, "{error}"),
            Self::OpenAiAdapter(error) => write!(f, "{error}"),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl Error for CliSetupError {}

impl From<io::Error> for CliSetupError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProviderRegistryError> for CliSetupError {
    fn from(value: ProviderRegistryError) -> Self {
        Self::Registry(value)
    }
}

#[derive(Debug)]
pub enum CliLoopError {
    Io(io::Error),
    Runtime(RuntimeError),
    Session(SessionTapeError),
}

impl std::fmt::Display for CliLoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Runtime(error) => write!(f, "{error}"),
            Self::Session(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CliLoopError {}

impl From<io::Error> for CliLoopError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<RuntimeError> for CliLoopError {
    fn from(value: RuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<SessionTapeError> for CliLoopError {
    fn from(value: SessionTapeError) -> Self {
        Self::Session(value)
    }
}
