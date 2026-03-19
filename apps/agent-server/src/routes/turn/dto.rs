use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct TurnRequest {
    pub prompt: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CancelTurnRequest {
    pub session_id: Option<String>,
}
