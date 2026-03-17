use rusqlite::{Row, types::Type};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{LlmTraceEvent, LlmTraceListItem, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus};

pub(super) fn read_trace_list_item(row: &Row<'_>) -> rusqlite::Result<LlmTraceListItem> {
    let provider_request = json_column::<Value>(row, 22)?;
    Ok(LlmTraceListItem {
        id: row.get(0)?,
        trace_id: row.get(1)?,
        span_id: row.get(2)?,
        parent_span_id: row.get(3)?,
        root_span_id: row.get(4)?,
        operation_name: row.get(5)?,
        span_kind: LlmTraceSpanKind::from_str(row.get::<_, String>(6)?.as_str()),
        turn_id: row.get(7)?,
        run_id: row.get(8)?,
        request_kind: row.get(9)?,
        step_index: row.get::<_, u32>(10)?,
        provider: row.get(11)?,
        protocol: row.get(12)?,
        model: row.get(13)?,
        endpoint_path: row.get(14)?,
        status: LlmTraceStatus::from_str(row.get::<_, String>(15)?.as_str()),
        stop_reason: row.get(16)?,
        status_code: row.get::<_, Option<u16>>(17)?,
        started_at_ms: row.get::<_, u64>(18)?,
        duration_ms: row.get::<_, Option<u64>>(19)?,
        total_tokens: row.get::<_, Option<u64>>(20)?,
        cached_tokens: row.get::<_, Option<u64>>(21)?,
        user_message: extract_user_message(&provider_request),
        error: row.get(23)?,
    })
}

pub(super) fn read_trace_record(row: &Row<'_>) -> rusqlite::Result<LlmTraceRecord> {
    Ok(LlmTraceRecord {
        id: row.get(0)?,
        trace_id: row.get(1)?,
        span_id: row.get(2)?,
        parent_span_id: row.get(3)?,
        root_span_id: row.get(4)?,
        operation_name: row.get(5)?,
        span_kind: LlmTraceSpanKind::from_str(row.get::<_, String>(6)?.as_str()),
        turn_id: row.get(7)?,
        run_id: row.get(8)?,
        request_kind: row.get(9)?,
        step_index: row.get::<_, u32>(10)?,
        provider: row.get(11)?,
        protocol: row.get(12)?,
        model: row.get(13)?,
        base_url: row.get(14)?,
        endpoint_path: row.get(15)?,
        streaming: row.get::<_, i64>(16)? != 0,
        started_at_ms: row.get::<_, u64>(17)?,
        finished_at_ms: row.get::<_, Option<u64>>(18)?,
        duration_ms: row.get::<_, Option<u64>>(19)?,
        status_code: row.get::<_, Option<u16>>(20)?,
        status: LlmTraceStatus::from_str(row.get::<_, String>(21)?.as_str()),
        stop_reason: row.get(22)?,
        error: row.get(23)?,
        request_summary: json_column(row, 24)?,
        provider_request: json_column(row, 25)?,
        response_summary: json_column(row, 26)?,
        response_body: row.get(27)?,
        input_tokens: row.get::<_, Option<u64>>(28)?,
        output_tokens: row.get::<_, Option<u64>>(29)?,
        total_tokens: row.get::<_, Option<u64>>(30)?,
        cached_tokens: row.get::<_, Option<u64>>(31)?,
        otel_attributes: json_column(row, 32)?,
        events: json_column::<Vec<LlmTraceEvent>>(row, 33)?,
    })
}

fn json_column<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    serde_json::from_str::<T>(&row.get::<_, String>(index)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn extract_user_message(value: &Value) -> Option<String> {
    extract_chat_completion_user_message(value)
        .or_else(|| extract_responses_user_message(value))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn extract_chat_completion_user_message(value: &Value) -> Option<String> {
    value.get("messages").and_then(Value::as_array).and_then(|messages| {
        messages.iter().rev().find_map(|message| {
            let role = message.get("role").and_then(Value::as_str)?;
            if role != "user" {
                return None;
            }
            extract_text_content(message.get("content")?)
        })
    })
}

fn extract_responses_user_message(value: &Value) -> Option<String> {
    value.get("input").and_then(Value::as_array).and_then(|items| {
        items.iter().rev().find_map(|item| {
            let role = item.get("role").and_then(Value::as_str)?;
            if role != "user" {
                return None;
            }
            extract_text_content(item.get("content")?)
        })
    })
}

fn extract_text_content(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| match item {
                    Value::String(text) => Some(text.clone()),
                    Value::Object(map) => {
                        map.get("text").and_then(Value::as_str).map(ToOwned::to_owned).or_else(
                            || map.get("content").and_then(Value::as_str).map(ToOwned::to_owned),
                        )
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| map.get("content").and_then(Value::as_str).map(ToOwned::to_owned)),
        _ => None,
    }
}
