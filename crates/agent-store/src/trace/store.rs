use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};

use crate::{AiaStore, AiaStoreError};

use super::{
    LlmTraceListItem, LlmTraceListPage, LlmTraceOverview, LlmTraceRecord, LlmTraceStore,
    LlmTraceSummary,
    mapping::{read_trace_list_item, read_trace_record},
};

impl LlmTraceStore for AiaStore {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            record_with_conn(conn, record)?;
            refresh_summary_snapshot_with_conn(conn, Some(record.request_kind.as_str()))?;
            refresh_summary_snapshot_with_conn(conn, None)?;
            Ok(())
        })
    }

    fn list(&self, limit: usize) -> Result<Vec<LlmTraceListItem>, AiaStoreError> {
        Ok(self.list_page(limit, 0)?.items)
    }

    fn list_page(&self, limit: usize, offset: usize) -> Result<LlmTraceListPage, AiaStoreError> {
        self.with_conn(|conn| list_page_with_conn(conn, limit, offset, None))
    }

    fn get(&self, id: &str) -> Result<Option<LlmTraceRecord>, AiaStoreError> {
        self.with_conn(|conn| get_with_conn(conn, id))
    }

    fn summary(&self) -> Result<LlmTraceSummary, AiaStoreError> {
        self.with_conn(|conn| load_summary_snapshot_with_conn(conn, None))
    }
}

impl AiaStore {
    pub async fn record_async(
        self: &Arc<Self>,
        record: LlmTraceRecord,
    ) -> Result<(), AiaStoreError> {
        self.with_conn_async(move |conn| {
            record_with_conn(conn, &record)?;
            refresh_summary_snapshot_with_conn(conn, Some(record.request_kind.as_str()))?;
            refresh_summary_snapshot_with_conn(conn, None)?;
            Ok(())
        })
        .await
    }

    pub async fn list_page_async(
        self: &Arc<Self>,
        limit: usize,
        offset: usize,
    ) -> Result<LlmTraceListPage, AiaStoreError> {
        self.with_conn_async(move |conn| list_page_with_conn(conn, limit, offset, None)).await
    }

    pub async fn list_page_by_request_kind_async(
        self: &Arc<Self>,
        limit: usize,
        offset: usize,
        request_kind: impl Into<String>,
    ) -> Result<LlmTraceListPage, AiaStoreError> {
        let request_kind = request_kind.into();
        self.with_conn_async(move |conn| {
            list_page_with_conn(conn, limit, offset, Some(request_kind.as_str()))
        })
        .await
    }

    pub async fn get_async(
        self: &Arc<Self>,
        id: impl Into<String>,
    ) -> Result<Option<LlmTraceRecord>, AiaStoreError> {
        let id = id.into();
        self.with_conn_async(move |conn| get_with_conn(conn, &id)).await
    }

    pub async fn summary_async(self: &Arc<Self>) -> Result<LlmTraceSummary, AiaStoreError> {
        self.with_conn_async(|conn| load_summary_snapshot_with_conn(conn, None)).await
    }

    pub async fn summary_by_request_kind_async(
        self: &Arc<Self>,
        request_kind: impl Into<String>,
    ) -> Result<LlmTraceSummary, AiaStoreError> {
        let request_kind = request_kind.into();
        self.with_conn_async(move |conn| {
            load_summary_snapshot_with_conn(conn, Some(request_kind.as_str()))
        })
        .await
    }

    pub async fn overview_by_request_kind_async(
        self: &Arc<Self>,
        limit: usize,
        offset: usize,
        request_kind: impl Into<String>,
    ) -> Result<LlmTraceOverview, AiaStoreError> {
        let request_kind = request_kind.into();
        self.with_conn_async(move |conn| {
            let page = list_page_with_conn(conn, limit, offset, Some(request_kind.as_str()))?;
            let summary = load_summary_snapshot_with_conn(conn, Some(request_kind.as_str()))?;
            Ok(LlmTraceOverview { summary, page })
        })
        .await
    }
}

fn record_with_conn(conn: &Connection, record: &LlmTraceRecord) -> Result<(), AiaStoreError> {
    conn.execute(
        "
        INSERT OR REPLACE INTO llm_request_traces (
            id, trace_id, span_id, parent_span_id, root_span_id,
            operation_name, span_kind, turn_id, run_id, request_kind,
            step_index, provider, protocol, model, base_url,
            endpoint_path, streaming, started_at_ms, finished_at_ms, duration_ms,
            status_code, status, stop_reason, error, request_summary,
            provider_request, response_summary, response_body, input_tokens, output_tokens,
            total_tokens, cached_tokens, otel_attributes, events
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25,
            ?26, ?27, ?28, ?29, ?30,
            ?31, ?32, ?33, ?34
        )
        ",
        params![
            record.id.as_str(),
            record.trace_id.as_str(),
            record.span_id.as_str(),
            record.parent_span_id.as_deref(),
            record.root_span_id.as_str(),
            record.operation_name.as_str(),
            record.span_kind.as_str(),
            record.turn_id.as_str(),
            record.run_id.as_str(),
            record.request_kind.as_str(),
            record.step_index,
            record.provider.as_str(),
            record.protocol.as_str(),
            record.model.as_str(),
            record.base_url.as_str(),
            record.endpoint_path.as_str(),
            record.streaming as i64,
            record.started_at_ms as i64,
            record.finished_at_ms.map(|value| value as i64),
            record.duration_ms.map(|value| value as i64),
            record.status_code.map(i64::from),
            record.status.as_str(),
            record.stop_reason.as_deref(),
            record.error.as_deref(),
            serde_json::to_string(&record.request_summary)?,
            serde_json::to_string(&record.provider_request)?,
            serde_json::to_string(&record.response_summary)?,
            record.response_body.as_deref(),
            record.input_tokens.map(|value| value as i64),
            record.output_tokens.map(|value| value as i64),
            record.total_tokens.map(|value| value as i64),
            record.cached_tokens.map(|value| value as i64),
            serde_json::to_string(&record.otel_attributes)?,
            serde_json::to_string(&record.events)?,
        ],
    )?;
    Ok(())
}

fn list_page_with_conn(
    conn: &Connection,
    limit: usize,
    offset: usize,
    request_kind: Option<&str>,
) -> Result<LlmTraceListPage, AiaStoreError> {
    let total_items = if let Some(request_kind) = request_kind {
        conn.query_row(
            "
            SELECT COUNT(*)
            FROM llm_request_traces
            WHERE span_kind = 'CLIENT' AND request_kind = ?1
            ",
            [request_kind],
            |row| row.get::<_, u64>(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM llm_request_traces WHERE span_kind = 'CLIENT'",
            [],
            |row| row.get::<_, u64>(0),
        )?
    };
    let mut stmt = if request_kind.is_some() {
        conn.prepare(
            "
            SELECT t.id, t.trace_id, t.span_id, t.parent_span_id, t.root_span_id,
                   t.operation_name, t.span_kind, t.turn_id, t.run_id, t.request_kind,
                   t.step_index, t.provider, t.protocol, t.model, t.endpoint_path,
                   t.status, t.stop_reason, t.status_code, t.started_at_ms, t.duration_ms,
                   t.total_tokens, t.cached_tokens, t.request_summary, t.error
            FROM llm_request_traces t
            WHERE t.span_kind = 'CLIENT' AND t.request_kind = ?1
            ORDER BY t.started_at_ms DESC, t.trace_id DESC, t.id DESC
            LIMIT ?2 OFFSET ?3
            ",
        )?
    } else {
        conn.prepare(
            "
            SELECT t.id, t.trace_id, t.span_id, t.parent_span_id, t.root_span_id,
                   t.operation_name, t.span_kind, t.turn_id, t.run_id, t.request_kind,
                   t.step_index, t.provider, t.protocol, t.model, t.endpoint_path,
                   t.status, t.stop_reason, t.status_code, t.started_at_ms, t.duration_ms,
                   t.total_tokens, t.cached_tokens, t.request_summary, t.error
            FROM llm_request_traces t
            WHERE t.span_kind = 'CLIENT'
            ORDER BY t.started_at_ms DESC, t.trace_id DESC, t.id DESC
            LIMIT ?1 OFFSET ?2
            ",
        )?
    };
    let rows = if let Some(request_kind) = request_kind {
        stmt.query_map(params![request_kind, limit as i64, offset as i64], read_trace_list_item)?
    } else {
        stmt.query_map(params![limit as i64, offset as i64], read_trace_list_item)?
    };

    Ok(LlmTraceListPage {
        items: rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)?,
        total_items,
        page: offset / limit + 1,
        page_size: limit,
    })
}

fn get_with_conn(conn: &Connection, id: &str) -> Result<Option<LlmTraceRecord>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "
        SELECT id, trace_id, span_id, parent_span_id, root_span_id,
               operation_name, span_kind, turn_id, run_id, request_kind,
               step_index, provider, protocol, model, base_url,
               endpoint_path, streaming, started_at_ms, finished_at_ms, duration_ms,
               status_code, status, stop_reason, error, request_summary,
               provider_request, response_summary, response_body, input_tokens, output_tokens,
               total_tokens, cached_tokens, otel_attributes, events
        FROM llm_request_traces
        WHERE id = ?1
        ",
    )?;

    stmt.query_row([id], read_trace_record).optional().map_err(AiaStoreError::from)
}

fn summary_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<LlmTraceSummary, AiaStoreError> {
    let (
        total_requests,
        failed_requests,
        total_input_tokens,
        total_output_tokens,
        total_tokens,
        total_cached_tokens,
    ): (u64, u64, u64, u64, u64, u64) = if let Some(request_kind) = request_kind {
        conn.query_row(
            "
                SELECT
                    COUNT(*),
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END),
                    SUM(COALESCE(input_tokens, 0)),
                    SUM(COALESCE(output_tokens, 0)),
                    SUM(COALESCE(total_tokens, 0)),
                    SUM(COALESCE(cached_tokens, 0))
                FROM llm_request_traces
                WHERE span_kind = 'CLIENT' AND request_kind = ?1
                ",
            [request_kind],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(3)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(4)?.unwrap_or(0),
                    row.get::<_, Option<u64>>(5)?.unwrap_or(0),
                ))
            },
        )?
    } else {
        conn.query_row(
            "
                SELECT
                    COUNT(*),
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END),
                    SUM(COALESCE(input_tokens, 0)),
                    SUM(COALESCE(output_tokens, 0)),
                    SUM(COALESCE(total_tokens, 0)),
                    SUM(COALESCE(cached_tokens, 0))
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
                    row.get::<_, Option<u64>>(5)?.unwrap_or(0),
                ))
            },
        )?
    };

    let durations = load_client_durations(conn, request_kind)?;
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
        total_cached_tokens,
    })
}

fn refresh_summary_snapshot_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<(), AiaStoreError> {
    let summary = summary_with_conn(conn, request_kind)?;
    let request_kind_key = request_kind.unwrap_or("*");
    let updated_at_ms = if let Some(request_kind) = request_kind {
        conn.query_row(
            "
            SELECT COALESCE(MAX(started_at_ms), 0)
            FROM llm_request_traces
            WHERE span_kind = 'CLIENT' AND request_kind = ?1
            ",
            [request_kind],
            |row| row.get::<_, u64>(0),
        )?
    } else {
        conn.query_row(
            "
            SELECT COALESCE(MAX(started_at_ms), 0)
            FROM llm_request_traces
            WHERE span_kind = 'CLIENT'
            ",
            [],
            |row| row.get::<_, u64>(0),
        )?
    };

    conn.execute(
        "
        INSERT INTO llm_trace_overview_summaries (
            request_kind, total_requests, failed_requests, avg_duration_ms, p95_duration_ms,
            total_input_tokens, total_output_tokens, total_tokens, total_cached_tokens, updated_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(request_kind) DO UPDATE SET
            total_requests = excluded.total_requests,
            failed_requests = excluded.failed_requests,
            avg_duration_ms = excluded.avg_duration_ms,
            p95_duration_ms = excluded.p95_duration_ms,
            total_input_tokens = excluded.total_input_tokens,
            total_output_tokens = excluded.total_output_tokens,
            total_tokens = excluded.total_tokens,
            total_cached_tokens = excluded.total_cached_tokens,
            updated_at_ms = excluded.updated_at_ms
        ",
        params![
            request_kind_key,
            summary.total_requests,
            summary.failed_requests,
            summary.avg_duration_ms,
            summary.p95_duration_ms,
            summary.total_input_tokens,
            summary.total_output_tokens,
            summary.total_tokens,
            summary.total_cached_tokens,
            updated_at_ms,
        ],
    )?;
    Ok(())
}

fn load_summary_snapshot_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<LlmTraceSummary, AiaStoreError> {
    let request_kind_key = request_kind.unwrap_or("*");
    let cached = conn
        .query_row(
            "
            SELECT total_requests, failed_requests, avg_duration_ms, p95_duration_ms,
                   total_input_tokens, total_output_tokens, total_tokens, total_cached_tokens
            FROM llm_trace_overview_summaries
            WHERE request_kind = ?1
            ",
            [request_kind_key],
            |row| {
                Ok(LlmTraceSummary {
                    total_requests: row.get(0)?,
                    failed_requests: row.get(1)?,
                    avg_duration_ms: row.get(2)?,
                    p95_duration_ms: row.get(3)?,
                    total_input_tokens: row.get(4)?,
                    total_output_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                    total_cached_tokens: row.get(7)?,
                })
            },
        )
        .optional()?;

    if let Some(summary) = cached {
        return Ok(summary);
    }

    refresh_summary_snapshot_with_conn(conn, request_kind)?;
    summary_with_conn(conn, request_kind)
}

fn load_client_durations(
    conn: &rusqlite::Connection,
    request_kind: Option<&str>,
) -> Result<Vec<u64>, AiaStoreError> {
    if let Some(request_kind) = request_kind {
        let mut stmt = conn.prepare(
            "
            SELECT duration_ms
            FROM llm_request_traces
            WHERE span_kind = 'CLIENT' AND request_kind = ?1 AND duration_ms IS NOT NULL
            ORDER BY duration_ms ASC
            ",
        )?;
        stmt.query_map([request_kind], |row| row.get::<_, u64>(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AiaStoreError::from)
    } else {
        let mut stmt = conn.prepare(
            "
            SELECT duration_ms
            FROM llm_request_traces
            WHERE span_kind = 'CLIENT' AND duration_ms IS NOT NULL
            ORDER BY duration_ms ASC
            ",
        )?;
        stmt.query_map([], |row| row.get::<_, u64>(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AiaStoreError::from)
    }
}
