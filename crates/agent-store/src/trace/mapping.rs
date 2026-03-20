use super::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus};
use rusqlite::{Row, types::Type};
use serde::de::DeserializeOwned;

pub(super) fn read_trace_record(row: &Row<'_>) -> rusqlite::Result<LlmTraceRecord> {
    Ok(LlmTraceRecord {
        id: row.get(0)?,
        trace_id: row.get(1)?,
        span_id: row.get(2)?,
        parent_span_id: row.get(3)?,
        root_span_id: row.get(4)?,
        operation_name: row.get(5)?,
        span_kind: LlmTraceSpanKind::from_str(row.get::<_, String>(6)?.as_str()),
        session_id: row.get(7)?,
        turn_id: row.get(8)?,
        run_id: row.get(9)?,
        request_kind: row.get(10)?,
        step_index: row.get::<_, u32>(11)?,
        provider: row.get(12)?,
        protocol: row.get(13)?,
        model: row.get(14)?,
        base_url: row.get(15)?,
        endpoint_path: row.get(16)?,
        streaming: row.get::<_, i64>(17)? != 0,
        started_at_ms: row.get::<_, u64>(18)?,
        finished_at_ms: row.get::<_, Option<u64>>(19)?,
        duration_ms: row.get::<_, Option<u64>>(20)?,
        status_code: row.get::<_, Option<u16>>(21)?,
        status: LlmTraceStatus::from_str(row.get::<_, String>(22)?.as_str()),
        stop_reason: row.get(23)?,
        error: row.get(24)?,
        request_summary: json_column(row, 25)?,
        provider_request: json_column(row, 26)?,
        response_summary: json_column(row, 27)?,
        response_body: row.get(28)?,
        input_tokens: row.get::<_, Option<u64>>(29)?,
        output_tokens: row.get::<_, Option<u64>>(30)?,
        total_tokens: row.get::<_, Option<u64>>(31)?,
        cached_tokens: row.get::<_, Option<u64>>(32)?,
        otel_attributes: json_column(row, 33)?,
        events: json_column::<Vec<LlmTraceEvent>>(row, 34)?,
    })
}

fn json_column<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    serde_json::from_str::<T>(&row.get::<_, String>(index)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}
