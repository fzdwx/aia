use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct CreateSessionRequest {
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct HandoffRequest {
    pub name: String,
    pub summary: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AutoCompressRequest {
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionQuery {
    pub session_id: Option<String>,
    pub before_turn_id: Option<String>,
    pub limit: Option<usize>,
}
