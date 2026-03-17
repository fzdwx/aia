use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::state::SharedState;

use super::common::{JsonResponse, error_response, json_response, trace_store_error_response};

#[derive(Deserialize)]
pub(crate) struct TraceListQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

pub(crate) async fn list_traces(
    State(state): State<SharedState>,
    Query(query): Query<TraceListQuery>,
) -> JsonResponse {
    let page_size = query.page_size.unwrap_or(12).clamp(1, 50);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * page_size;

    match state.store.list_page_async(page_size, offset).await {
        Ok(result) => json_response(StatusCode::OK, result),
        Err(error) => trace_store_error_response(error),
    }
}

pub(crate) async fn get_trace(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> JsonResponse {
    let missing_id = id.clone();
    match state.store.get_async(id).await {
        Ok(Some(trace)) => json_response(StatusCode::OK, trace),
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("trace 不存在：{missing_id}")),
        Err(error) => trace_store_error_response(error),
    }
}

pub(crate) async fn get_trace_summary(State(state): State<SharedState>) -> JsonResponse {
    match state.store.summary_async().await {
        Ok(summary) => json_response(StatusCode::OK, summary),
        Err(error) => trace_store_error_response(error),
    }
}
