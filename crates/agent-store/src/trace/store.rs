use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};

use crate::{AiaStore, AiaStoreError};

use super::{
    LlmTraceLoopDetail, LlmTraceLoopItem, LlmTraceLoopPage, LlmTraceLoopStatus, LlmTraceOverview,
    LlmTraceRecord, LlmTraceStatus, LlmTraceStore, LlmTraceSummary,
    mapping::{read_trace_list_item, read_trace_record},
};

impl LlmTraceStore for AiaStore {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            record_with_conn(conn, record)?;
            refresh_loop_snapshot_with_conn(conn, record.trace_id.as_str())?;
            refresh_summary_snapshot_with_conn(conn, Some(record.request_kind.as_str()))?;
            refresh_summary_snapshot_with_conn(conn, None)?;
            Ok(())
        })
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
            refresh_loop_snapshot_with_conn(conn, record.trace_id.as_str())?;
            refresh_summary_snapshot_with_conn(conn, Some(record.request_kind.as_str()))?;
            refresh_summary_snapshot_with_conn(conn, None)?;
            Ok(())
        })
        .await
    }

    pub async fn list_loop_page_async(
        self: &Arc<Self>,
        limit: usize,
        offset: usize,
    ) -> Result<LlmTraceLoopPage, AiaStoreError> {
        self.with_conn_async(move |conn| loop_page_with_conn(conn, limit, offset, None)).await
    }

    pub async fn list_loop_page_by_request_kind_async(
        self: &Arc<Self>,
        limit: usize,
        offset: usize,
        request_kind: impl Into<String>,
    ) -> Result<LlmTraceLoopPage, AiaStoreError> {
        let request_kind = request_kind.into();
        self.with_conn_async(move |conn| {
            loop_page_with_conn(conn, limit, offset, Some(request_kind.as_str()))
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

    pub async fn get_loop_async(
        self: &Arc<Self>,
        id: impl Into<String>,
    ) -> Result<Option<LlmTraceLoopDetail>, AiaStoreError> {
        let id = id.into();
        self.with_conn_async(move |conn| get_loop_with_conn(conn, &id)).await
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
            let page = loop_page_with_conn(conn, limit, offset, Some(request_kind.as_str()))?;
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

fn loop_page_with_conn(
    conn: &Connection,
    limit: usize,
    offset: usize,
    request_kind: Option<&str>,
) -> Result<LlmTraceLoopPage, AiaStoreError> {
    let total_items = if let Some(request_kind) = request_kind {
        conn.query_row(
            "SELECT COUNT(*) FROM llm_trace_loops WHERE request_kind = ?1",
            [request_kind],
            |row| row.get::<_, u64>(0),
        )?
    } else {
        conn.query_row("SELECT COUNT(*) FROM llm_trace_loops", [], |row| row.get::<_, u64>(0))?
    };

    let mut stmt = if request_kind.is_some() {
        conn.prepare(
            "
            SELECT id, trace_id, request_kind, turn_id, run_id, root_span_id,
                   model, protocol, endpoint_path, latest_started_at_ms, started_at_ms,
                   finished_at_ms, duration_ms, total_tokens, total_cached_tokens,
                   llm_span_count, tool_span_count, failed_tool_count, final_status,
                   user_message, latest_error, final_span_id, traces_json
            FROM llm_trace_loops
            WHERE request_kind = ?1
            ORDER BY latest_started_at_ms DESC, trace_id DESC
            LIMIT ?2 OFFSET ?3
            ",
        )?
    } else {
        conn.prepare(
            "
            SELECT id, trace_id, request_kind, turn_id, run_id, root_span_id,
                   model, protocol, endpoint_path, latest_started_at_ms, started_at_ms,
                   finished_at_ms, duration_ms, total_tokens, total_cached_tokens,
                   llm_span_count, tool_span_count, failed_tool_count, final_status,
                   user_message, latest_error, final_span_id, traces_json
            FROM llm_trace_loops
            ORDER BY latest_started_at_ms DESC, trace_id DESC
            LIMIT ?1 OFFSET ?2
            ",
        )?
    };

    let rows = if let Some(request_kind) = request_kind {
        stmt.query_map(params![request_kind, limit as i64, offset as i64], read_trace_loop_item)?
    } else {
        stmt.query_map(params![limit as i64, offset as i64], read_trace_loop_item)?
    };

    Ok(LlmTraceLoopPage {
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

fn get_loop_with_conn(
    conn: &Connection,
    id: &str,
) -> Result<Option<LlmTraceLoopDetail>, AiaStoreError> {
    let loop_item = conn
        .query_row(
            "
            SELECT id, trace_id, request_kind, turn_id, run_id, root_span_id,
                   model, protocol, endpoint_path, latest_started_at_ms, started_at_ms,
                   finished_at_ms, duration_ms, total_tokens, total_cached_tokens,
                   llm_span_count, tool_span_count, failed_tool_count, final_status,
                   user_message, latest_error, final_span_id, traces_json
            FROM llm_trace_loops
            WHERE id = ?1 OR trace_id = ?1
            LIMIT 1
            ",
            [id],
            read_trace_loop_item,
        )
        .optional()?;

    let Some(loop_item) = loop_item else {
        return Ok(None);
    };

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
        WHERE trace_id = ?1
        ORDER BY started_at_ms ASC, request_kind ASC, id ASC
        ",
    )?;

    let trace_details = stmt
        .query_map([loop_item.trace_id.as_str()], read_trace_record)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    Ok(Some(LlmTraceLoopDetail { loop_item, trace_details }))
}

fn summary_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<LlmTraceSummary, AiaStoreError> {
    let (total_requests, failed_requests, total_tokens, total_cached_tokens): (u64, u64, u64, u64) =
        if let Some(request_kind) = request_kind {
            conn.query_row(
                "
                SELECT
                    COUNT(*),
                    SUM(CASE WHEN final_status = 'failed' THEN 1 ELSE 0 END),
                    SUM(COALESCE(total_tokens, 0)),
                    SUM(COALESCE(total_cached_tokens, 0))
                FROM llm_trace_loops
                WHERE request_kind = ?1
                ",
                [request_kind],
                |row| {
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<u64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<u64>>(3)?.unwrap_or(0),
                    ))
                },
            )?
        } else {
            conn.query_row(
                "
                SELECT
                    COUNT(*),
                    SUM(CASE WHEN final_status = 'failed' THEN 1 ELSE 0 END),
                    SUM(COALESCE(total_tokens, 0)),
                    SUM(COALESCE(total_cached_tokens, 0))
                FROM llm_trace_loops
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<u64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<u64>>(3)?.unwrap_or(0),
                    ))
                },
            )?
        };

    let (total_input_tokens, total_output_tokens): (u64, u64) =
        if let Some(request_kind) = request_kind {
            conn.query_row(
                "
                SELECT
                    SUM(COALESCE(input_tokens, 0)),
                    SUM(COALESCE(output_tokens, 0))
                FROM llm_request_traces
                WHERE span_kind = 'CLIENT' AND request_kind = ?1
                ",
                [request_kind],
                |row| {
                    Ok((
                        row.get::<_, Option<u64>>(0)?.unwrap_or(0),
                        row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                    ))
                },
            )?
        } else {
            conn.query_row(
                "
                SELECT
                    SUM(COALESCE(input_tokens, 0)),
                    SUM(COALESCE(output_tokens, 0))
                FROM llm_request_traces
                WHERE span_kind = 'CLIENT'
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<u64>>(0)?.unwrap_or(0),
                        row.get::<_, Option<u64>>(1)?.unwrap_or(0),
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

fn refresh_loop_snapshot_with_conn(conn: &Connection, trace_id: &str) -> Result<(), AiaStoreError> {
    let mut stmt = conn.prepare(
        "
        SELECT t.id, t.trace_id, t.span_id, t.parent_span_id, t.root_span_id,
               t.operation_name, t.span_kind, t.turn_id, t.run_id, t.request_kind,
               t.step_index, t.provider, t.protocol, t.model, t.endpoint_path,
               t.status, t.stop_reason, t.status_code, t.started_at_ms, t.duration_ms,
               t.total_tokens, t.cached_tokens, t.request_summary, t.error
        FROM llm_request_traces t
        WHERE t.trace_id = ?1
        ORDER BY t.started_at_ms ASC, t.request_kind ASC, t.id ASC
        ",
    )?;

    let traces = stmt
        .query_map([trace_id], read_trace_list_item)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    if traces.is_empty() {
        conn.execute("DELETE FROM llm_trace_loops WHERE trace_id = ?1", [trace_id])?;
        return Ok(());
    }

    let llm_traces =
        traces.iter().filter(|trace| trace.request_kind != "tool").cloned().collect::<Vec<_>>();
    let tool_traces =
        traces.iter().filter(|trace| trace.request_kind == "tool").cloned().collect::<Vec<_>>();
    let latest_trace = traces
        .iter()
        .max_by(|left, right| {
            left.started_at_ms.cmp(&right.started_at_ms).then_with(|| left.id.cmp(&right.id))
        })
        .cloned()
        .ok_or_else(|| AiaStoreError::new("trace loop latest item missing"))?;
    let latest_llm_trace = llm_traces
        .iter()
        .max_by(|left, right| {
            left.started_at_ms.cmp(&right.started_at_ms).then_with(|| left.id.cmp(&right.id))
        })
        .cloned();
    let representative = latest_llm_trace.clone().unwrap_or_else(|| latest_trace.clone());

    let started_at_ms = traces.iter().map(|trace| trace.started_at_ms).min().unwrap_or(0);
    let latest_started_at_ms = latest_trace.started_at_ms;
    let finished_at_ms = traces
        .iter()
        .filter_map(|trace| {
            trace.duration_ms.map(|duration| trace.started_at_ms.saturating_add(duration))
        })
        .max();
    let duration_ms = finished_at_ms.map(|finished| finished.saturating_sub(started_at_ms));
    let total_tokens = llm_traces.iter().map(|trace| trace.total_tokens.unwrap_or(0)).sum::<u64>();
    let total_cached_tokens =
        llm_traces.iter().map(|trace| trace.cached_tokens.unwrap_or(0)).sum::<u64>();
    let llm_span_count = llm_traces.len() as u32;
    let tool_span_count = tool_traces.len() as u32;
    let failed_tool_count =
        tool_traces.iter().filter(|trace| trace.status == LlmTraceStatus::Failed).count() as u32;
    let has_llm_failure = llm_traces.iter().any(|trace| trace.status == LlmTraceStatus::Failed);
    let final_llm_trace = llm_traces.last().cloned();
    let final_status = match final_llm_trace.as_ref() {
        Some(trace) if trace.status == LlmTraceStatus::Failed => LlmTraceLoopStatus::Failed,
        _ if has_llm_failure || failed_tool_count > 0 => LlmTraceLoopStatus::Partial,
        _ => LlmTraceLoopStatus::Completed,
    };
    let user_message = latest_llm_trace
        .as_ref()
        .and_then(|trace| trace.user_message.clone())
        .or_else(|| latest_trace.user_message.clone());
    let latest_error = traces.iter().rev().find_map(|trace| trace.error.clone());
    let final_span_id = traces.last().map(|trace| trace.id.clone());
    let traces_json = serde_json::to_string(&traces)?;

    conn.execute(
        "
        INSERT INTO llm_trace_loops (
            id, trace_id, request_kind, turn_id, run_id, root_span_id,
            model, protocol, endpoint_path, latest_started_at_ms, started_at_ms,
            finished_at_ms, duration_ms, total_tokens, total_cached_tokens,
            llm_span_count, tool_span_count, failed_tool_count, final_status,
            user_message, latest_error, final_span_id, traces_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19,
            ?20, ?21, ?22, ?23
        )
        ON CONFLICT(id) DO UPDATE SET
            trace_id = excluded.trace_id,
            request_kind = excluded.request_kind,
            turn_id = excluded.turn_id,
            run_id = excluded.run_id,
            root_span_id = excluded.root_span_id,
            model = excluded.model,
            protocol = excluded.protocol,
            endpoint_path = excluded.endpoint_path,
            latest_started_at_ms = excluded.latest_started_at_ms,
            started_at_ms = excluded.started_at_ms,
            finished_at_ms = excluded.finished_at_ms,
            duration_ms = excluded.duration_ms,
            total_tokens = excluded.total_tokens,
            total_cached_tokens = excluded.total_cached_tokens,
            llm_span_count = excluded.llm_span_count,
            tool_span_count = excluded.tool_span_count,
            failed_tool_count = excluded.failed_tool_count,
            final_status = excluded.final_status,
            user_message = excluded.user_message,
            latest_error = excluded.latest_error,
            final_span_id = excluded.final_span_id,
            traces_json = excluded.traces_json
        ",
        params![
            trace_id,
            trace_id,
            representative.request_kind,
            representative.turn_id,
            representative.run_id,
            representative.root_span_id,
            representative.model,
            representative.protocol,
            representative.endpoint_path,
            latest_started_at_ms,
            started_at_ms,
            finished_at_ms,
            duration_ms,
            total_tokens,
            total_cached_tokens,
            llm_span_count,
            tool_span_count,
            failed_tool_count,
            final_status.as_str(),
            user_message,
            latest_error,
            final_span_id,
            traces_json,
        ],
    )?;

    Ok(())
}

fn read_trace_loop_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LlmTraceLoopItem> {
    Ok(LlmTraceLoopItem {
        id: row.get(0)?,
        trace_id: row.get(1)?,
        request_kind: row.get(2)?,
        turn_id: row.get(3)?,
        run_id: row.get(4)?,
        root_span_id: row.get(5)?,
        model: row.get(6)?,
        protocol: row.get(7)?,
        endpoint_path: row.get(8)?,
        latest_started_at_ms: row.get(9)?,
        started_at_ms: row.get(10)?,
        finished_at_ms: row.get(11)?,
        duration_ms: row.get(12)?,
        total_tokens: row.get(13)?,
        total_cached_tokens: row.get(14)?,
        llm_span_count: row.get(15)?,
        tool_span_count: row.get(16)?,
        failed_tool_count: row.get(17)?,
        final_status: LlmTraceLoopStatus::from_str(row.get::<_, String>(18)?.as_str()),
        user_message: row.get(19)?,
        latest_error: row.get(20)?,
        final_span_id: row.get(21)?,
        traces: serde_json::from_str(&row.get::<_, String>(22)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                22,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
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
            FROM llm_trace_loops
            WHERE request_kind = ?1 AND duration_ms IS NOT NULL
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
            FROM llm_trace_loops
            WHERE duration_ms IS NOT NULL
            ORDER BY duration_ms ASC
            ",
        )?;
        stmt.query_map([], |row| row.get::<_, u64>(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AiaStoreError::from)
    }
}
