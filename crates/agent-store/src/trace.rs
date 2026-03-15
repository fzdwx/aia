use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AiaStore, AiaStoreError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmTraceStatus {
    Succeeded,
    Failed,
}

impl LlmTraceStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "failed" => Self::Failed,
            _ => Self::Succeeded,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LlmTraceSpanKind {
    Client,
    Internal,
}

impl LlmTraceSpanKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Client => "CLIENT",
            Self::Internal => "INTERNAL",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "INTERNAL" | "internal" => Self::Internal,
            _ => Self::Client,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceEvent {
    pub name: String,
    pub at_ms: u64,
    pub attributes: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceRecord {
    pub id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub root_span_id: String,
    pub operation_name: String,
    pub span_kind: LlmTraceSpanKind,
    pub turn_id: String,
    pub run_id: String,
    pub request_kind: String,
    pub step_index: u32,
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub base_url: String,
    pub endpoint_path: String,
    pub streaming: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub status: LlmTraceStatus,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
    pub request_summary: Value,
    pub provider_request: Value,
    pub response_summary: Value,
    pub response_body: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub otel_attributes: Value,
    pub events: Vec<LlmTraceEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceListItem {
    pub id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub root_span_id: String,
    pub operation_name: String,
    pub span_kind: LlmTraceSpanKind,
    pub turn_id: String,
    pub run_id: String,
    pub request_kind: String,
    pub step_index: u32,
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub endpoint_path: String,
    pub status: LlmTraceStatus,
    pub stop_reason: Option<String>,
    pub status_code: Option<u16>,
    pub started_at_ms: u64,
    pub duration_ms: Option<u64>,
    pub total_tokens: Option<u64>,
    pub user_message: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceSummary {
    pub total_requests: u64,
    pub failed_requests: u64,
    pub avg_duration_ms: Option<f64>,
    pub p95_duration_ms: Option<u64>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTraceListPage {
    pub items: Vec<LlmTraceListItem>,
    pub total_loops: u64,
    pub page: usize,
    pub page_size: usize,
}

pub trait LlmTraceStore: Send + Sync {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), AiaStoreError>;
    fn list(&self, limit: usize) -> Result<Vec<LlmTraceListItem>, AiaStoreError>;
    fn list_page(&self, limit: usize, offset: usize) -> Result<LlmTraceListPage, AiaStoreError>;
    fn get(&self, id: &str) -> Result<Option<LlmTraceRecord>, AiaStoreError>;
    fn summary(&self) -> Result<LlmTraceSummary, AiaStoreError>;
}

impl AiaStore {
    pub(crate) fn init_trace_schema(&self) -> Result<(), AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS llm_request_traces (
                id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                span_id TEXT NOT NULL,
                parent_span_id TEXT,
                root_span_id TEXT NOT NULL,
                operation_name TEXT NOT NULL,
                span_kind TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                request_kind TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                provider TEXT NOT NULL,
                protocol TEXT NOT NULL,
                model TEXT NOT NULL,
                base_url TEXT NOT NULL,
                endpoint_path TEXT NOT NULL,
                streaming INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                finished_at_ms INTEGER,
                duration_ms INTEGER,
                status_code INTEGER,
                status TEXT NOT NULL,
                stop_reason TEXT,
                error TEXT,
                request_summary TEXT NOT NULL,
                provider_request TEXT NOT NULL,
                response_summary TEXT NOT NULL,
                response_body TEXT,
                input_tokens INTEGER,
                output_tokens INTEGER,
                total_tokens INTEGER,
                otel_attributes TEXT NOT NULL DEFAULT '{}',
                events TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_llm_request_traces_started_at_ms
                ON llm_request_traces(started_at_ms DESC);
            ",
        )?;

        ensure_column(&conn, "llm_request_traces", "trace_id", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "llm_request_traces", "span_id", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "llm_request_traces", "parent_span_id", "TEXT")?;
        ensure_column(&conn, "llm_request_traces", "root_span_id", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "llm_request_traces", "operation_name", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "llm_request_traces", "span_kind", "TEXT NOT NULL DEFAULT 'CLIENT'")?;
        ensure_column(
            &conn,
            "llm_request_traces",
            "otel_attributes",
            "TEXT NOT NULL DEFAULT '{}'",
        )?;
        ensure_column(&conn, "llm_request_traces", "events", "TEXT NOT NULL DEFAULT '[]'")?;

        Ok(())
    }
}

impl LlmTraceStore for AiaStore {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), AiaStoreError> {
        self.conn.lock().expect("lock poisoned").execute(
            "
            INSERT OR REPLACE INTO llm_request_traces (
                id, trace_id, span_id, parent_span_id, root_span_id,
                operation_name, span_kind, turn_id, run_id, request_kind,
                step_index, provider, protocol, model, base_url,
                endpoint_path, streaming, started_at_ms, finished_at_ms, duration_ms,
                status_code, status, stop_reason, error, request_summary,
                provider_request, response_summary, response_body, input_tokens, output_tokens,
                total_tokens, otel_attributes, events
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25,
                ?26, ?27, ?28, ?29, ?30,
                ?31, ?32, ?33
            )
            ",
            params![
                record.id,
                record.trace_id,
                record.span_id,
                record.parent_span_id,
                record.root_span_id,
                record.operation_name,
                record.span_kind.as_str(),
                record.turn_id,
                record.run_id,
                record.request_kind,
                record.step_index,
                record.provider,
                record.protocol,
                record.model,
                record.base_url,
                record.endpoint_path,
                record.streaming as i64,
                record.started_at_ms as i64,
                record.finished_at_ms.map(|value| value as i64),
                record.duration_ms.map(|value| value as i64),
                record.status_code.map(i64::from),
                record.status.as_str(),
                record.stop_reason,
                record.error,
                serde_json::to_string(&record.request_summary)?,
                serde_json::to_string(&record.provider_request)?,
                serde_json::to_string(&record.response_summary)?,
                record.response_body,
                record.input_tokens.map(|value| value as i64),
                record.output_tokens.map(|value| value as i64),
                record.total_tokens.map(|value| value as i64),
                serde_json::to_string(&record.otel_attributes)?,
                serde_json::to_string(&record.events)?,
            ],
        )?;
        Ok(())
    }

    fn list(&self, limit: usize) -> Result<Vec<LlmTraceListItem>, AiaStoreError> {
        Ok(self.list_page(limit, 0)?.items)
    }

    fn list_page(&self, limit: usize, offset: usize) -> Result<LlmTraceListPage, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let total_loops =
            conn.query_row("SELECT COUNT(DISTINCT trace_id) FROM llm_request_traces", [], |row| {
                row.get::<_, u64>(0)
            })?;
        let mut stmt = conn.prepare(
            "
            WITH paged_loops AS (
                SELECT trace_id, MAX(started_at_ms) AS latest_started_at_ms
                FROM llm_request_traces
                GROUP BY trace_id
                ORDER BY latest_started_at_ms DESC, trace_id DESC
                LIMIT ?1 OFFSET ?2
            )
            SELECT t.id, t.trace_id, t.span_id, t.parent_span_id, t.root_span_id,
                   t.operation_name, t.span_kind, t.turn_id, t.run_id, t.request_kind,
                   t.step_index, t.provider, t.protocol, t.model, t.endpoint_path,
                   t.status, t.stop_reason, t.status_code, t.started_at_ms, t.duration_ms,
                   t.total_tokens, t.provider_request, t.error
            FROM llm_request_traces t
            JOIN paged_loops p ON p.trace_id = t.trace_id
            ORDER BY p.latest_started_at_ms DESC, t.started_at_ms DESC, t.id DESC
            ",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            let provider_request = serde_json::from_str::<Value>(&row.get::<_, String>(21)?)
                .map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        21,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
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
                user_message: extract_user_message(&provider_request),
                error: row.get(22)?,
            })
        })?;
        Ok(LlmTraceListPage {
            items: rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)?,
            total_loops,
            page: offset / limit + 1,
            page_size: limit,
        })
    }

    fn get(&self, id: &str) -> Result<Option<LlmTraceRecord>, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let mut stmt = conn.prepare(
            "
            SELECT id, trace_id, span_id, parent_span_id, root_span_id,
                   operation_name, span_kind, turn_id, run_id, request_kind,
                   step_index, provider, protocol, model, base_url,
                   endpoint_path, streaming, started_at_ms, finished_at_ms, duration_ms,
                   status_code, status, stop_reason, error, request_summary,
                   provider_request, response_summary, response_body, input_tokens, output_tokens,
                   total_tokens, otel_attributes, events
            FROM llm_request_traces
            WHERE id = ?1
            ",
        )?;

        stmt.query_row([id], |row| {
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
                request_summary: serde_json::from_str::<Value>(&row.get::<_, String>(24)?)
                    .map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            24,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                provider_request: serde_json::from_str::<Value>(&row.get::<_, String>(25)?)
                    .map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            25,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                response_summary: serde_json::from_str::<Value>(&row.get::<_, String>(26)?)
                    .map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            26,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                response_body: row.get(27)?,
                input_tokens: row.get::<_, Option<u64>>(28)?,
                output_tokens: row.get::<_, Option<u64>>(29)?,
                total_tokens: row.get::<_, Option<u64>>(30)?,
                otel_attributes: serde_json::from_str::<Value>(&row.get::<_, String>(31)?)
                    .map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            31,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                events: serde_json::from_str::<Vec<LlmTraceEvent>>(&row.get::<_, String>(32)?)
                    .map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            32,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
            })
        })
        .optional()
        .map_err(AiaStoreError::from)
    }

    fn summary(&self) -> Result<LlmTraceSummary, AiaStoreError> {
        let conn = self.conn.lock().expect("lock poisoned");
        let (
            total_requests,
            failed_requests,
            total_input_tokens,
            total_output_tokens,
            total_tokens,
        ): (u64, u64, u64, u64, u64) = conn.query_row(
            "
                SELECT
                    COUNT(*),
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END),
                    SUM(COALESCE(input_tokens, 0)),
                    SUM(COALESCE(output_tokens, 0)),
                    SUM(COALESCE(total_tokens, 0))
                FROM llm_request_traces
                WHERE span_kind = 'CLIENT'
                ",
            [],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(3)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(4)?.unwrap_or(0),
                ))
            },
        )?;

        let mut stmt = conn.prepare(
            "SELECT duration_ms FROM llm_request_traces WHERE span_kind = 'CLIENT' AND duration_ms IS NOT NULL ORDER BY duration_ms ASC",
        )?;
        let durations =
            stmt.query_map([], |row| row.get::<_, u64>(0))?.collect::<Result<Vec<_>, _>>()?;

        let avg_duration_ms = if durations.is_empty() {
            None
        } else {
            Some(durations.iter().sum::<u64>() as f64 / durations.len() as f64)
        };
        let p95_duration_ms = if durations.is_empty() {
            None
        } else {
            let index = ((durations.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
            durations.get(index).copied().or_else(|| durations.last().copied())
        };

        Ok(LlmTraceSummary {
            total_requests,
            failed_requests,
            avg_duration_ms,
            p95_duration_ms,
            total_input_tokens,
            total_output_tokens,
            total_tokens,
        })
    }
}

fn ensure_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AiaStoreError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let existing =
        stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>()?;
    if existing.iter().any(|name| name == column) {
        return Ok(());
    }

    let alter = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    conn.execute(&alter, [])?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{LlmTraceEvent, LlmTraceRecord, LlmTraceSpanKind, LlmTraceStatus, LlmTraceStore};
    use crate::AiaStore;

    #[test]
    fn store_records_round_trip_and_summary() {
        let store = AiaStore::in_memory().expect("store should initialize");
        let record = LlmTraceRecord {
            id: "trace-1".into(),
            trace_id: "trace-group-1".into(),
            span_id: "trace-1".into(),
            parent_span_id: Some("root-span-1".into()),
            root_span_id: "root-span-1".into(),
            operation_name: "chat".into(),
            span_kind: LlmTraceSpanKind::Client,
            turn_id: "turn-1".into(),
            run_id: "turn-1".into(),
            request_kind: "completion".into(),
            step_index: 0,
            provider: "openai".into(),
            protocol: "openai-responses".into(),
            model: "gpt-5.4".into(),
            base_url: "https://api.example.com".into(),
            endpoint_path: "/responses".into(),
            streaming: true,
            started_at_ms: 100,
            finished_at_ms: Some(180),
            duration_ms: Some(80),
            status_code: Some(200),
            status: LlmTraceStatus::Succeeded,
            stop_reason: Some("stop".into()),
            error: None,
            request_summary: json!({"conversation_items": 2}),
            provider_request: json!({"model": "gpt-5.4"}),
            response_summary: json!({"assistant_text": "你好"}),
            response_body: Some("你好".into()),
            input_tokens: Some(12),
            output_tokens: Some(6),
            total_tokens: Some(18),
            otel_attributes: json!({"gen_ai.operation.name": "chat"}),
            events: vec![LlmTraceEvent {
                name: "response.completed".into(),
                at_ms: 180,
                attributes: json!({"http.response.status_code": 200}),
            }],
        };

        store.record(&record).expect("record should persist");

        let loaded = store.get("trace-1").expect("query should succeed").expect("trace exists");
        assert_eq!(loaded, record);

        let list = store.list(10).expect("list should succeed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "trace-1");
        assert_eq!(list[0].status, LlmTraceStatus::Succeeded);
        assert_eq!(list[0].stop_reason.as_deref(), Some("stop"));
        assert_eq!(list[0].total_tokens, Some(18));
        assert_eq!(list[0].user_message, None);

        let summary = store.summary().expect("summary should succeed");
        assert_eq!(summary.total_requests, 1);
        assert_eq!(summary.failed_requests, 0);
        assert_eq!(summary.total_tokens, 18);
        assert_eq!(summary.p95_duration_ms, Some(80));
    }

    #[test]
    fn list_extracts_user_message_from_chat_completions_request() {
        let store = AiaStore::in_memory().expect("store should initialize");
        store
            .record(&LlmTraceRecord {
                id: "trace-chat".into(),
                trace_id: "trace-chat-group".into(),
                span_id: "trace-chat".into(),
                parent_span_id: Some("trace-chat-root".into()),
                root_span_id: "trace-chat-root".into(),
                operation_name: "chat".into(),
                span_kind: LlmTraceSpanKind::Client,
                turn_id: "turn-chat".into(),
                run_id: "turn-chat".into(),
                request_kind: "completion".into(),
                step_index: 0,
                provider: "openai".into(),
                protocol: "openai-chat-completions".into(),
                model: "gpt-5.4".into(),
                base_url: "https://api.example.com".into(),
                endpoint_path: "/chat/completions".into(),
                streaming: false,
                started_at_ms: 100,
                finished_at_ms: Some(180),
                duration_ms: Some(80),
                status_code: Some(200),
                status: LlmTraceStatus::Succeeded,
                stop_reason: Some("stop".into()),
                error: None,
                request_summary: json!({}),
                provider_request: json!({
                    "messages": [
                        {"role": "system", "content": "keep it short"},
                        {"role": "user", "content": "summarize this repo"}
                    ]
                }),
                response_summary: json!({}),
                response_body: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                otel_attributes: json!({"gen_ai.operation.name": "chat"}),
                events: vec![],
            })
            .expect("record should persist");

        let list = store.list(10).expect("list should succeed");
        assert_eq!(list[0].user_message.as_deref(), Some("summarize this repo"));
    }

    #[test]
    fn list_extracts_user_message_from_responses_request() {
        let store = AiaStore::in_memory().expect("store should initialize");
        store
            .record(&LlmTraceRecord {
                id: "trace-responses".into(),
                trace_id: "trace-responses-group".into(),
                span_id: "trace-responses".into(),
                parent_span_id: Some("trace-responses-root".into()),
                root_span_id: "trace-responses-root".into(),
                operation_name: "chat".into(),
                span_kind: LlmTraceSpanKind::Client,
                turn_id: "turn-responses".into(),
                run_id: "turn-responses".into(),
                request_kind: "completion".into(),
                step_index: 0,
                provider: "openai".into(),
                protocol: "openai-responses".into(),
                model: "gpt-5.4".into(),
                base_url: "https://api.example.com".into(),
                endpoint_path: "/responses".into(),
                streaming: false,
                started_at_ms: 100,
                finished_at_ms: Some(180),
                duration_ms: Some(80),
                status_code: Some(200),
                status: LlmTraceStatus::Succeeded,
                stop_reason: Some("stop".into()),
                error: None,
                request_summary: json!({}),
                provider_request: json!({
                    "input": [
                        {"role": "system", "content": "keep it short"},
                        {"role": "user", "content": "explain the failing test"}
                    ]
                }),
                response_summary: json!({}),
                response_body: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                otel_attributes: json!({"gen_ai.operation.name": "chat"}),
                events: vec![],
            })
            .expect("record should persist");

        let list = store.list(10).expect("list should succeed");
        assert_eq!(list[0].user_message.as_deref(), Some("explain the failing test"));
    }

    #[test]
    fn list_page_paginates_by_loop_not_individual_span() {
        let store = AiaStore::in_memory().expect("store should initialize");

        for (loop_index, started_at_ms) in [(1_u32, 300_u64), (2_u32, 200_u64), (3_u32, 100_u64)] {
            for step_index in 0..2_u32 {
                store
                    .record(&LlmTraceRecord {
                        id: format!("trace-{loop_index}-{step_index}"),
                        trace_id: format!("loop-{loop_index}"),
                        span_id: format!("span-{loop_index}-{step_index}"),
                        parent_span_id: Some(format!("root-{loop_index}")),
                        root_span_id: format!("root-{loop_index}"),
                        operation_name: "chat".into(),
                        span_kind: LlmTraceSpanKind::Client,
                        turn_id: format!("turn-{loop_index}"),
                        run_id: format!("run-{loop_index}"),
                        request_kind: "completion".into(),
                        step_index,
                        provider: "openai".into(),
                        protocol: "openai-responses".into(),
                        model: "gpt-5".into(),
                        base_url: "https://api.example.com".into(),
                        endpoint_path: "/responses".into(),
                        streaming: false,
                        started_at_ms: started_at_ms + u64::from(step_index),
                        finished_at_ms: Some(started_at_ms + u64::from(step_index) + 10),
                        duration_ms: Some(10),
                        status_code: Some(200),
                        status: LlmTraceStatus::Succeeded,
                        stop_reason: Some("stop".into()),
                        error: None,
                        request_summary: json!({}),
                        provider_request: json!({
                            "input": [
                                {"role": "user", "content": format!("message {loop_index}")}
                            ]
                        }),
                        response_summary: json!({}),
                        response_body: None,
                        input_tokens: None,
                        output_tokens: None,
                        total_tokens: None,
                        otel_attributes: json!({}),
                        events: vec![],
                    })
                    .expect("record should persist");
            }
        }

        let first_page = store.list_page(2, 0).expect("page query should succeed");
        assert_eq!(first_page.total_loops, 3);
        assert_eq!(first_page.page, 1);
        assert_eq!(first_page.page_size, 2);
        assert_eq!(first_page.items.len(), 4);
        assert!(
            first_page
                .items
                .iter()
                .all(|item| item.trace_id == "loop-1" || item.trace_id == "loop-2")
        );
        assert!(first_page.items.iter().any(|item| item.trace_id == "loop-1"));
        assert!(first_page.items.iter().any(|item| item.trace_id == "loop-2"));
        assert!(first_page.items.iter().all(|item| item.trace_id != "loop-3"));

        let second_page = store.list_page(2, 2).expect("page query should succeed");
        assert_eq!(second_page.total_loops, 3);
        assert_eq!(second_page.page, 2);
        assert_eq!(second_page.page_size, 2);
        assert_eq!(second_page.items.len(), 2);
        assert!(second_page.items.iter().all(|item| item.trace_id == "loop-3"));
    }
}
