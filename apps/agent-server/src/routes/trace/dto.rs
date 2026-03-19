use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct TraceListQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub request_kind: Option<String>,
}
