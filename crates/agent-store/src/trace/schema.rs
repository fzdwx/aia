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
                    session_id TEXT,
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
                CREATE INDEX IF NOT EXISTS idx_llm_request_traces_client_kind_trace_started
                    ON llm_request_traces(span_kind, request_kind, trace_id, started_at_ms DESC);
                CREATE INDEX IF NOT EXISTS idx_llm_request_traces_trace_id_started_at_ms
                    ON llm_request_traces(trace_id, started_at_ms DESC);
                CREATE INDEX IF NOT EXISTS idx_llm_request_traces_client_kind_duration_ms
                    ON llm_request_traces(span_kind, request_kind, duration_ms);
                CREATE INDEX IF NOT EXISTS idx_llm_request_traces_client_request_trace_started_partial
                    ON llm_request_traces(request_kind, trace_id, started_at_ms DESC)
                    WHERE span_kind = 'CLIENT';
                CREATE INDEX IF NOT EXISTS idx_llm_request_traces_client_request_duration_partial
                    ON llm_request_traces(request_kind, duration_ms)
                    WHERE span_kind = 'CLIENT';

                CREATE TABLE IF NOT EXISTS llm_trace_overview_summaries (
                    request_kind TEXT PRIMARY KEY,
                    total_requests INTEGER NOT NULL DEFAULT 0,
                    failed_requests INTEGER NOT NULL DEFAULT 0,
                    partial_requests INTEGER NOT NULL DEFAULT 0,
                    avg_duration_ms REAL,
                    p95_duration_ms INTEGER,
                    total_llm_spans INTEGER NOT NULL DEFAULT 0,
                    total_tool_spans INTEGER NOT NULL DEFAULT 0,
                    requests_with_tools INTEGER NOT NULL DEFAULT 0,
                    failed_tool_calls INTEGER NOT NULL DEFAULT 0,
                    unique_models INTEGER NOT NULL DEFAULT 0,
                    latest_request_started_at_ms INTEGER,
                    total_input_tokens INTEGER NOT NULL DEFAULT 0,
                    total_output_tokens INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    total_cached_tokens INTEGER NOT NULL DEFAULT 0,
                    total_duration_ms INTEGER NOT NULL DEFAULT 0,
                    duration_sample_count INTEGER NOT NULL DEFAULT 0,
                    updated_at_ms INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS llm_trace_summary_model_counts (
                    request_kind TEXT NOT NULL,
                    model TEXT NOT NULL,
                    loop_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (request_kind, model)
                );

                CREATE TABLE IF NOT EXISTS llm_trace_summary_duration_buckets (
                    request_kind TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    sample_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (request_kind, duration_ms)
                );

                CREATE TABLE IF NOT EXISTS llm_trace_dirty_loops (
                    trace_id TEXT PRIMARY KEY,
                    updated_at_ms INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS llm_trace_loops (
                    id TEXT PRIMARY KEY,
                    trace_id TEXT NOT NULL,
                    request_kind TEXT NOT NULL,
                    session_id TEXT NOT NULL DEFAULT '',
                    turn_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    root_span_id TEXT NOT NULL,
                    model TEXT NOT NULL,
                    protocol TEXT NOT NULL,
                    endpoint_path TEXT NOT NULL,
                    latest_started_at_ms INTEGER NOT NULL,
                    started_at_ms INTEGER NOT NULL,
                    finished_at_ms INTEGER,
                    duration_ms INTEGER,
                    total_input_tokens INTEGER NOT NULL DEFAULT 0,
                    total_output_tokens INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    total_cached_tokens INTEGER NOT NULL DEFAULT 0,
                    estimated_cost_micros INTEGER NOT NULL DEFAULT 0,
                    lines_added INTEGER NOT NULL DEFAULT 0,
                    lines_removed INTEGER NOT NULL DEFAULT 0,
                    llm_span_count INTEGER NOT NULL DEFAULT 0,
                    tool_span_count INTEGER NOT NULL DEFAULT 0,
                    failed_tool_count INTEGER NOT NULL DEFAULT 0,
                    final_status TEXT NOT NULL,
                    user_message TEXT,
                    latest_error TEXT,
                    final_span_id TEXT,
                    traces_json TEXT NOT NULL DEFAULT '[]'
                );
                CREATE INDEX IF NOT EXISTS idx_llm_trace_loops_request_kind_latest_started
                    ON llm_trace_loops(request_kind, latest_started_at_ms DESC, trace_id DESC);
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
            ensure_column(conn, "llm_request_traces", "session_id", "TEXT")?;
            ensure_column(
                conn,
                "llm_request_traces",
                "otel_attributes",
                "TEXT NOT NULL DEFAULT '{}'",
            )?;
            ensure_column(conn, "llm_request_traces", "events", "TEXT NOT NULL DEFAULT '[]'")?;
            ensure_column(conn, "llm_request_traces", "cached_tokens", "INTEGER")?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "partial_requests",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "total_llm_spans",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "total_tool_spans",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "requests_with_tools",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "failed_tool_calls",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "unique_models",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "latest_request_started_at_ms",
                "INTEGER",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "total_duration_ms",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_overview_summaries",
                "duration_sample_count",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_loops",
                "session_id",
                "TEXT NOT NULL DEFAULT ''",
            )?;
            ensure_column(
                conn,
                "llm_trace_loops",
                "total_input_tokens",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_loops",
                "total_output_tokens",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_loops",
                "estimated_cost_micros",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_loops",
                "lines_added",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "llm_trace_loops",
                "lines_removed",
                "INTEGER NOT NULL DEFAULT 0",
            )?;

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
