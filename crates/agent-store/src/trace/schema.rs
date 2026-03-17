use rusqlite::Connection;

use crate::{AiaStore, AiaStoreError};

impl AiaStore {
    pub(crate) fn init_trace_schema(&self) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
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
                    cached_tokens INTEGER,
                    otel_attributes TEXT NOT NULL DEFAULT '{}',
                    events TEXT NOT NULL DEFAULT '[]'
                );
                CREATE INDEX IF NOT EXISTS idx_llm_request_traces_started_at_ms
                    ON llm_request_traces(started_at_ms DESC);
                ",
            )?;

            ensure_column(conn, "llm_request_traces", "trace_id", "TEXT NOT NULL DEFAULT ''")?;
            ensure_column(conn, "llm_request_traces", "span_id", "TEXT NOT NULL DEFAULT ''")?;
            ensure_column(conn, "llm_request_traces", "parent_span_id", "TEXT")?;
            ensure_column(conn, "llm_request_traces", "root_span_id", "TEXT NOT NULL DEFAULT ''")?;
            ensure_column(
                conn,
                "llm_request_traces",
                "operation_name",
                "TEXT NOT NULL DEFAULT ''",
            )?;
            ensure_column(
                conn,
                "llm_request_traces",
                "span_kind",
                "TEXT NOT NULL DEFAULT 'CLIENT'",
            )?;
            ensure_column(
                conn,
                "llm_request_traces",
                "otel_attributes",
                "TEXT NOT NULL DEFAULT '{}'",
            )?;
            ensure_column(conn, "llm_request_traces", "events", "TEXT NOT NULL DEFAULT '[]'")?;
            ensure_column(conn, "llm_request_traces", "cached_tokens", "INTEGER")?;

            Ok(())
        })
    }
}

fn ensure_column(
    conn: &Connection,
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
