use axum::{Json, body::Bytes, extract::State, response::IntoResponse};

use crate::{channel_runtime::handle_feishu_webhook, state::SharedState};

use super::common::error_response;

pub(crate) async fn feishu_event(
    State(state): State<SharedState>,
    body: Bytes,
) -> impl IntoResponse {
    match handle_feishu_webhook(state.as_ref(), &body).await {
        Ok(payload) => Json(payload).into_response(),
        Err(error) => error_response(axum::http::StatusCode::BAD_REQUEST, error).into_response(),
    }
}
