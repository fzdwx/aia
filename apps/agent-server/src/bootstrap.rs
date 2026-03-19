mod startup;

use std::sync::Arc;

use crate::state::AppState;

pub fn build_server_user_agent() -> String {
    aia_config::build_user_agent(aia_config::APP_NAME, env!("CARGO_PKG_VERSION"))
}

#[derive(Debug)]
pub struct ServerInitError {
    step: &'static str,
    message: String,
}

impl ServerInitError {
    pub fn new(step: &'static str, message: impl Into<String>) -> Self {
        Self { step, message: message.into() }
    }
}

impl std::fmt::Display for ServerInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{step}失败: {message}", step = self.step, message = self.message)
    }
}

impl std::error::Error for ServerInitError {}

pub async fn bootstrap_state() -> Result<Arc<AppState>, ServerInitError> {
    startup::ServerBootstrap::discover()?.bootstrap().await
}
