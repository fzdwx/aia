use axum::{Router, routing::post};
use serde::{Deserialize, Serialize};

use crate::state::SharedState;

mod handlers;

#[derive(Deserialize)]
#[serde(tag = "mode")]
pub(crate) enum DiffRequest {
    #[serde(rename = "contents")]
    Contents {
        file_name: String,
        old_content: String,
        new_content: String,
        #[serde(default)]
        theme: Option<String>,
    },
    #[serde(rename = "patch")]
    Patch {
        patch: String,
        #[serde(default)]
        theme: Option<String>,
    },
}

#[derive(Serialize)]
pub(crate) struct DiffResponse {
    pub hunks: Vec<DiffHunk>,
}

#[derive(Serialize)]
pub(crate) struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Serialize)]
pub(crate) struct DiffLine {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_ln: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_ln: Option<u32>,
    pub html: String,
}

pub(crate) fn router() -> Router<SharedState> {
    Router::new().route("/api/diff", post(handlers::compute_diff))
}
