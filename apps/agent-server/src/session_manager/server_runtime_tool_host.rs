use agent_core::{CoreError, QuestionRequest, QuestionResult, RuntimeToolHost};
use async_trait::async_trait;

use crate::session_manager::{
    RuntimeWorkerError,
    types::{RuntimeToolHost as RuntimeToolHostHandle, SessionCommand},
};

#[derive(Clone)]
pub(crate) struct ServerRuntimeToolHost {
    runtime_tool_host: std::sync::Arc<RuntimeToolHostHandle>,
}

impl ServerRuntimeToolHost {
    pub(crate) fn new(runtime_tool_host: std::sync::Arc<RuntimeToolHostHandle>) -> Self {
        Self { runtime_tool_host }
    }
}

#[async_trait]
impl RuntimeToolHost for ServerRuntimeToolHost {
    async fn ask_question(
        &self,
        session_id: &str,
        request: QuestionRequest,
    ) -> Result<QuestionResult, CoreError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.runtime_tool_host
            .tx
            .send(SessionCommand::AskQuestion {
                session_id: session_id.to_string(),
                request,
                reply: reply_tx,
            })
            .await
            .map_err(|_| CoreError::new("question coordinator unavailable"))?;

        reply_rx
            .await
            .map_err(|_| CoreError::new("question coordinator dropped"))?
            .map_err(|error: RuntimeWorkerError| CoreError::new(error.message))
    }
}
