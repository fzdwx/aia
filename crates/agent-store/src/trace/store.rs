use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{AiaStore, AiaStoreError};

use super::{
    LlmTraceLoopDetail, LlmTraceLoopItem, LlmTraceLoopPage, LlmTraceLoopStatus, LlmTraceOverview,
    LlmTraceRecord, LlmTraceStatus, LlmTraceStore, LlmTraceSummary, mapping::read_trace_record,
};

#[derive(Debug, Clone, PartialEq)]
struct LoopTraceRollupRow {
    id: String,
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    root_span_id: String,
    operation_name: String,
    span_kind: super::LlmTraceSpanKind,
    session_id: Option<String>,
    turn_id: String,
    run_id: String,
    request_kind: String,
    step_index: u32,
    provider: String,
    protocol: String,
    model: String,
    endpoint_path: String,
    status: LlmTraceStatus,
    stop_reason: Option<String>,
    status_code: Option<u16>,
    started_at_ms: u64,
    duration_ms: Option<u64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    cached_tokens: Option<u64>,
    user_message: Option<String>,
    error: Option<String>,
    otel_attributes: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoopSummarySnapshot {
    trace_id: String,
    request_kind: String,
    session_id: String,
    model: String,
    latest_started_at_ms: u64,
    duration_ms: Option<u64>,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_tokens: u64,
    total_cached_tokens: u64,
    estimated_cost_micros: u64,
    lines_added: u64,
    lines_removed: u64,
    llm_span_count: u64,
    tool_span_count: u64,
    failed_tool_count: u64,
    final_status: LlmTraceLoopStatus,
}

#[derive(Debug, Clone, PartialEq)]
struct SummaryRollupRow {
    summary: LlmTraceSummary,
    total_duration_ms: u64,
    duration_sample_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoopSummaryChange {
    previous: Option<LoopSummarySnapshot>,
    current: Option<LoopSummarySnapshot>,
}

impl LlmTraceStore for AiaStore {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), AiaStoreError> {
        self.with_conn(|conn| {
            with_write_transaction(conn, |conn| record_trace_with_rollups(conn, record))
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
            with_write_transaction(conn, |conn| record_trace_with_rollups(conn, &record))
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
            operation_name, span_kind, session_id, turn_id, run_id, request_kind,
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
            ?31, ?32, ?33, ?34, ?35
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
            record.session_id.as_deref(),
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

fn record_trace_with_rollups(
    conn: &Connection,
    record: &LlmTraceRecord,
) -> Result<(), AiaStoreError> {
    ensure_summary_rollup_state_with_conn(conn)?;
    mark_trace_loop_dirty_with_conn(conn, record.trace_id.as_str(), record.started_at_ms)?;
    record_with_conn(conn, record)?;
    let change = refresh_loop_snapshot_with_conn(conn, record.trace_id.as_str())?;
    refresh_summary_rollups_with_conn(conn, &change)?;
    clear_trace_loop_dirty_with_conn(conn, record.trace_id.as_str())
}

fn mark_trace_loop_dirty_with_conn(
    conn: &Connection,
    trace_id: &str,
    updated_at_ms: u64,
) -> Result<(), AiaStoreError> {
    conn.execute(
        "
        INSERT INTO llm_trace_dirty_loops (trace_id, updated_at_ms)
        VALUES (?1, ?2)
        ON CONFLICT(trace_id) DO UPDATE SET updated_at_ms = excluded.updated_at_ms
        ",
        params![trace_id, updated_at_ms],
    )?;
    Ok(())
}

fn clear_trace_loop_dirty_with_conn(
    conn: &Connection,
    trace_id: &str,
) -> Result<(), AiaStoreError> {
    conn.execute("DELETE FROM llm_trace_dirty_loops WHERE trace_id = ?1", [trace_id])?;
    Ok(())
}

pub(super) fn enqueue_legacy_trace_loop_backfill_with_conn(
    conn: &Connection,
) -> Result<(), AiaStoreError> {
    conn.execute(
        "
        INSERT OR IGNORE INTO llm_trace_dirty_loops (trace_id, updated_at_ms)
        SELECT trace_id, latest_started_at_ms
        FROM llm_trace_loops
        WHERE session_id = ''
        ",
        [],
    )?;
    Ok(())
}

pub(super) fn reconcile_dirty_trace_loops_with_conn(
    conn: &Connection,
) -> Result<(), AiaStoreError> {
    let mut stmt = conn.prepare(
        "
        SELECT trace_id
        FROM llm_trace_dirty_loops
        ORDER BY updated_at_ms ASC, trace_id ASC
        ",
    )?;
    let trace_ids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    for trace_id in trace_ids {
        let change = refresh_loop_snapshot_with_conn(conn, trace_id.as_str())?;
        refresh_summary_rollups_with_conn(conn, &change)?;
        clear_trace_loop_dirty_with_conn(conn, trace_id.as_str())?;
    }

    Ok(())
}

pub(super) fn with_write_transaction<R>(
    conn: &Connection,
    action: impl FnOnce(&Connection) -> Result<R, AiaStoreError>,
) -> Result<R, AiaStoreError> {
    conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
    match action(conn) {
        Ok(value) => {
            conn.execute_batch("COMMIT")?;
            Ok(value)
        }
        Err(error) => {
            let rollback_result = conn.execute_batch("ROLLBACK");
            if let Err(rollback_error) = rollback_result {
                return Err(AiaStoreError::new(format!(
                    "transaction failed: {error}; rollback failed: {rollback_error}"
                )));
            }
            Err(error)
        }
    }
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
                   finished_at_ms, duration_ms, total_input_tokens, total_output_tokens,
                   total_tokens, total_cached_tokens,
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
                   finished_at_ms, duration_ms, total_input_tokens, total_output_tokens,
                   total_tokens, total_cached_tokens,
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
               operation_name, span_kind, session_id, turn_id, run_id, request_kind,
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
                   finished_at_ms, duration_ms, total_input_tokens, total_output_tokens,
                   total_tokens, total_cached_tokens,
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
               operation_name, span_kind, session_id, turn_id, run_id, request_kind,
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

pub(super) fn refresh_loop_snapshot_with_conn(
    conn: &Connection,
    trace_id: &str,
) -> Result<LoopSummaryChange, AiaStoreError> {
    let previous = load_loop_summary_snapshot_with_conn(conn, trace_id)?;
    let mut stmt = conn.prepare(
        "
        SELECT t.id, t.trace_id, t.span_id, t.parent_span_id, t.root_span_id,
               t.operation_name, t.span_kind, t.session_id, t.turn_id, t.run_id, t.request_kind,
               t.step_index, t.provider, t.protocol, t.model, t.endpoint_path,
               t.status, t.stop_reason, t.status_code, t.started_at_ms, t.duration_ms,
               t.input_tokens, t.output_tokens, t.total_tokens, t.cached_tokens,
               t.request_summary, t.error, t.otel_attributes
        FROM llm_request_traces t
        WHERE t.trace_id = ?1
        ORDER BY t.started_at_ms ASC, t.request_kind ASC, t.id ASC
        ",
    )?;

    let traces = stmt
        .query_map([trace_id], read_loop_trace_rollup_row)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    if traces.is_empty() {
        conn.execute("DELETE FROM llm_trace_loops WHERE trace_id = ?1", [trace_id])?;
        return Ok(LoopSummaryChange { previous, current: None });
    }

    let llm_traces =
        traces.iter().filter(|trace| trace.request_kind != "tool").cloned().collect::<Vec<_>>();
    let tool_traces =
        traces.iter().filter(|trace| trace.request_kind == "tool").cloned().collect::<Vec<_>>();
    ensure_loop_rollup_invariants(trace_id, &llm_traces)?;
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
    let duration_ms = finished_at_ms.map(|finished: u64| finished.saturating_sub(started_at_ms));
    let total_input_tokens =
        llm_traces.iter().map(|trace| trace.input_tokens.unwrap_or(0)).sum::<u64>();
    let total_output_tokens =
        llm_traces.iter().map(|trace| trace.output_tokens.unwrap_or(0)).sum::<u64>();
    let total_tokens = llm_traces.iter().map(|trace| trace.total_tokens.unwrap_or(0)).sum::<u64>();
    let total_cached_tokens =
        llm_traces.iter().map(|trace| trace.cached_tokens.unwrap_or(0)).sum::<u64>();
    let estimated_cost_micros = llm_traces
        .iter()
        .map(|trace| {
            estimate_trace_cost_micros(
                trace.model.as_str(),
                trace.input_tokens,
                trace.output_tokens,
                trace.cached_tokens,
            )
        })
        .sum::<u64>();
    let (lines_added, lines_removed) =
        tool_traces.iter().fold((0_u64, 0_u64), |(added, removed), trace| {
            let (trace_added, trace_removed) =
                extract_tool_line_changes(trace.model.as_str(), &trace.otel_attributes);
            (added.saturating_add(trace_added), removed.saturating_add(trace_removed))
        });
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
    let session_id = latest_llm_trace
        .as_ref()
        .and_then(|trace| trace.session_id.clone())
        .or_else(|| latest_trace.session_id.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| representative.turn_id.clone());
    let latest_error = traces.iter().rev().find_map(|trace| trace.error.clone());
    let final_span_id = traces.last().map(|trace| trace.id.clone());
    let traces_json = serde_json::to_string(
        &traces.iter().cloned().map(loop_trace_rollup_row_to_list_item).collect::<Vec<_>>(),
    )?;

    conn.execute(
        "
        INSERT INTO llm_trace_loops (
            id, trace_id, request_kind, session_id, turn_id, run_id, root_span_id,
            model, protocol, endpoint_path, latest_started_at_ms, started_at_ms,
            finished_at_ms, duration_ms, total_input_tokens, total_output_tokens,
            total_tokens, total_cached_tokens, estimated_cost_micros, lines_added, lines_removed,
            llm_span_count, tool_span_count, failed_tool_count, final_status,
            user_message, latest_error, final_span_id, traces_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19,
            ?20, ?21, ?22, ?23,
            ?24, ?25, ?26, ?27,
            ?28, ?29
        )
        ON CONFLICT(id) DO UPDATE SET
            trace_id = excluded.trace_id,
            request_kind = excluded.request_kind,
            session_id = excluded.session_id,
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
            total_input_tokens = excluded.total_input_tokens,
            total_output_tokens = excluded.total_output_tokens,
            total_tokens = excluded.total_tokens,
            total_cached_tokens = excluded.total_cached_tokens,
            estimated_cost_micros = excluded.estimated_cost_micros,
            lines_added = excluded.lines_added,
            lines_removed = excluded.lines_removed,
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
            session_id,
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
            total_input_tokens,
            total_output_tokens,
            total_tokens,
            total_cached_tokens,
            estimated_cost_micros,
            lines_added,
            lines_removed,
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

    Ok(LoopSummaryChange {
        previous,
        current: Some(LoopSummarySnapshot {
            trace_id: trace_id.to_string(),
            request_kind: representative.request_kind.clone(),
            session_id,
            model: representative.model.clone(),
            latest_started_at_ms,
            duration_ms,
            total_input_tokens,
            total_output_tokens,
            total_tokens,
            total_cached_tokens,
            estimated_cost_micros,
            lines_added,
            lines_removed,
            llm_span_count: u64::from(llm_span_count),
            tool_span_count: u64::from(tool_span_count),
            failed_tool_count: u64::from(failed_tool_count),
            final_status,
        }),
    })
}

fn read_loop_trace_rollup_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LoopTraceRollupRow> {
    let request_summary =
        serde_json::from_str::<Value>(&row.get::<_, String>(25)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                25,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let otel_attributes =
        serde_json::from_str::<Value>(&row.get::<_, String>(27)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                27,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(LoopTraceRollupRow {
        id: row.get(0)?,
        trace_id: row.get(1)?,
        span_id: row.get(2)?,
        parent_span_id: row.get(3)?,
        root_span_id: row.get(4)?,
        operation_name: row.get(5)?,
        span_kind: super::LlmTraceSpanKind::from_str(row.get::<_, String>(6)?.as_str()),
        session_id: row.get(7)?,
        turn_id: row.get(8)?,
        run_id: row.get(9)?,
        request_kind: row.get(10)?,
        step_index: row.get(11)?,
        provider: row.get(12)?,
        protocol: row.get(13)?,
        model: row.get(14)?,
        endpoint_path: row.get(15)?,
        status: LlmTraceStatus::from_str(row.get::<_, String>(16)?.as_str()),
        stop_reason: row.get(17)?,
        status_code: row.get(18)?,
        started_at_ms: row.get(19)?,
        duration_ms: row.get(20)?,
        input_tokens: row.get(21)?,
        output_tokens: row.get(22)?,
        total_tokens: row.get(23)?,
        cached_tokens: row.get(24)?,
        user_message: extract_user_message_from_request_summary(&request_summary),
        error: row.get(26)?,
        otel_attributes,
    })
}

fn loop_trace_rollup_row_to_list_item(trace: LoopTraceRollupRow) -> super::LlmTraceListItem {
    super::LlmTraceListItem {
        id: trace.id,
        trace_id: trace.trace_id,
        span_id: trace.span_id,
        parent_span_id: trace.parent_span_id,
        root_span_id: trace.root_span_id,
        operation_name: trace.operation_name,
        span_kind: trace.span_kind,
        turn_id: trace.turn_id,
        run_id: trace.run_id,
        request_kind: trace.request_kind,
        step_index: trace.step_index,
        provider: trace.provider,
        protocol: trace.protocol,
        model: trace.model,
        endpoint_path: trace.endpoint_path,
        status: trace.status,
        stop_reason: trace.stop_reason,
        status_code: trace.status_code,
        started_at_ms: trace.started_at_ms,
        duration_ms: trace.duration_ms,
        total_tokens: trace.total_tokens,
        cached_tokens: trace.cached_tokens,
        user_message: trace.user_message,
        error: trace.error,
    }
}

fn extract_user_message_from_request_summary(value: &Value) -> Option<String> {
    value
        .get("user_message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn estimate_trace_cost_micros(
    model: &str,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_tokens: Option<u64>,
) -> u64 {
    let pricing = model_pricing_for(model);
    let input_tokens = input_tokens.unwrap_or(0);
    let output_tokens = output_tokens.unwrap_or(0);
    let cached_tokens = cached_tokens.unwrap_or(0).min(input_tokens);
    let billable_input_tokens = input_tokens.saturating_sub(cached_tokens);

    (((billable_input_tokens as u128) * (pricing.input_micros_per_million as u128))
        + ((cached_tokens as u128) * (pricing.cached_input_micros_per_million as u128))
        + ((output_tokens as u128) * (pricing.output_micros_per_million as u128)))
        .checked_div(1_000_000)
        .unwrap_or(0) as u64
}

struct ModelPricing {
    input_micros_per_million: u64,
    cached_input_micros_per_million: u64,
    output_micros_per_million: u64,
}

fn model_pricing_for(model: &str) -> ModelPricing {
    if model.contains("mini") {
        return ModelPricing {
            input_micros_per_million: 250_000,
            cached_input_micros_per_million: 25_000,
            output_micros_per_million: 2_000_000,
        };
    }

    ModelPricing {
        input_micros_per_million: 1_250_000,
        cached_input_micros_per_million: 125_000,
        output_micros_per_million: 10_000_000,
    }
}

fn extract_tool_line_changes(model: &str, otel_attributes: &Value) -> (u64, u64) {
    let details = otel_attributes
        .get("aia.tool.details")
        .and_then(|value| if value.is_object() { Some(value) } else { None });
    let Some(details) = details else {
        return (0, 0);
    };

    if let Some(added) = details.get("lines_added").and_then(Value::as_u64) {
        let removed = details.get("lines_removed").and_then(Value::as_u64).unwrap_or(0);
        return (added, removed);
    }

    if let Some(added) = details.get("added").and_then(Value::as_u64) {
        let removed = details.get("removed").and_then(Value::as_u64).unwrap_or(0);
        return (added, removed);
    }

    if model == "write" {
        let lines = details.get("lines").and_then(Value::as_u64).unwrap_or(0);
        return (lines, 0);
    }

    (0, 0)
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
        total_tokens: row.get(15)?,
        total_cached_tokens: row.get(16)?,
        llm_span_count: row.get(17)?,
        tool_span_count: row.get(18)?,
        failed_tool_count: row.get(19)?,
        final_status: LlmTraceLoopStatus::from_str(row.get::<_, String>(20)?.as_str()),
        user_message: row.get(21)?,
        latest_error: row.get(22)?,
        final_span_id: row.get(23)?,
        traces: serde_json::from_str(&row.get::<_, String>(24)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                24,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

pub(super) fn load_summary_snapshot_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<LlmTraceSummary, AiaStoreError> {
    ensure_summary_rollup_state_with_conn(conn)?;
    let request_kind_key = request_kind.unwrap_or("*");
    let cached = conn
        .query_row(
            "
            SELECT total_requests, failed_requests, partial_requests, avg_duration_ms,
                   p95_duration_ms, total_llm_spans, total_tool_spans, requests_with_tools,
                   failed_tool_calls, unique_models, latest_request_started_at_ms,
                   total_input_tokens, total_output_tokens, total_tokens, total_cached_tokens
            FROM llm_trace_overview_summaries
            WHERE request_kind = ?1
            ",
            [request_kind_key],
            |row| {
                Ok(LlmTraceSummary {
                    total_requests: row.get(0)?,
                    failed_requests: row.get(1)?,
                    partial_requests: row.get(2)?,
                    avg_duration_ms: row.get(3)?,
                    p95_duration_ms: row.get(4)?,
                    total_llm_spans: row.get(5)?,
                    total_tool_spans: row.get(6)?,
                    requests_with_tools: row.get(7)?,
                    failed_tool_calls: row.get(8)?,
                    unique_models: row.get(9)?,
                    latest_request_started_at_ms: row.get(10)?,
                    total_input_tokens: row.get(11)?,
                    total_output_tokens: row.get(12)?,
                    total_tokens: row.get(13)?,
                    total_cached_tokens: row.get(14)?,
                })
            },
        )
        .optional()?;

    if let Some(summary) = cached {
        return Ok(summary);
    }

    Ok(LlmTraceSummary {
        total_requests: 0,
        failed_requests: 0,
        partial_requests: 0,
        avg_duration_ms: None,
        p95_duration_ms: None,
        total_llm_spans: 0,
        total_tool_spans: 0,
        requests_with_tools: 0,
        failed_tool_calls: 0,
        unique_models: 0,
        latest_request_started_at_ms: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_tokens: 0,
        total_cached_tokens: 0,
    })
}

fn ensure_summary_rollup_state_with_conn(conn: &Connection) -> Result<(), AiaStoreError> {
    let has_loops =
        conn.query_row("SELECT EXISTS(SELECT 1 FROM llm_trace_loops LIMIT 1)", [], |row| {
            row.get::<_, i64>(0)
        })? != 0;
    if !has_loops {
        return Ok(());
    }

    let has_global_summary = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM llm_trace_overview_summaries WHERE request_kind = '*')",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;
    let has_model_counts = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM llm_trace_summary_model_counts LIMIT 1)",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;
    let needs_duration_buckets = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM llm_trace_loops WHERE duration_ms IS NOT NULL LIMIT 1)",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;
    let has_duration_buckets = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM llm_trace_summary_duration_buckets LIMIT 1)",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;

    if has_global_summary && has_model_counts && (!needs_duration_buckets || has_duration_buckets) {
        return Ok(());
    }

    with_write_transaction(conn, |conn| {
        backfill_loop_input_output_tokens_with_conn(conn)?;
        rebuild_summary_rollup_state_with_conn(conn)
    })
}

fn backfill_loop_input_output_tokens_with_conn(conn: &Connection) -> Result<(), AiaStoreError> {
    conn.execute(
        "
        UPDATE llm_trace_loops
        SET total_input_tokens = COALESCE((
                SELECT SUM(COALESCE(t.input_tokens, 0))
                FROM llm_request_traces t
                WHERE t.trace_id = llm_trace_loops.trace_id AND t.span_kind = 'CLIENT'
            ), 0),
            total_output_tokens = COALESCE((
                SELECT SUM(COALESCE(t.output_tokens, 0))
                FROM llm_request_traces t
                WHERE t.trace_id = llm_trace_loops.trace_id AND t.span_kind = 'CLIENT'
            ), 0)
        ",
        [],
    )?;
    Ok(())
}

fn rebuild_summary_rollup_state_with_conn(conn: &Connection) -> Result<(), AiaStoreError> {
    conn.execute("DELETE FROM llm_trace_overview_summaries", [])?;
    conn.execute("DELETE FROM llm_trace_summary_model_counts", [])?;
    conn.execute("DELETE FROM llm_trace_summary_duration_buckets", [])?;

    let mut stmt = conn.prepare(
        "
        SELECT trace_id, request_kind, session_id, model, latest_started_at_ms, duration_ms,
               total_input_tokens, total_output_tokens, total_tokens, total_cached_tokens,
               estimated_cost_micros, lines_added, lines_removed,
               llm_span_count, tool_span_count, failed_tool_count, final_status
        FROM llm_trace_loops
        ",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LoopSummarySnapshot {
                trace_id: row.get(0)?,
                request_kind: row.get(1)?,
                session_id: row.get(2)?,
                model: row.get(3)?,
                latest_started_at_ms: row.get(4)?,
                duration_ms: row.get(5)?,
                total_input_tokens: row.get(6)?,
                total_output_tokens: row.get(7)?,
                total_tokens: row.get(8)?,
                total_cached_tokens: row.get(9)?,
                estimated_cost_micros: row.get(10)?,
                lines_added: row.get(11)?,
                lines_removed: row.get(12)?,
                llm_span_count: row.get(13)?,
                tool_span_count: row.get(14)?,
                failed_tool_count: row.get(15)?,
                final_status: LlmTraceLoopStatus::from_str(row.get::<_, String>(16)?.as_str()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    for snapshot in rows {
        refresh_summary_rollups_with_conn(
            conn,
            &LoopSummaryChange { previous: None, current: Some(snapshot) },
        )?;
    }

    Ok(())
}

fn load_loop_summary_snapshot_with_conn(
    conn: &Connection,
    trace_id: &str,
) -> Result<Option<LoopSummarySnapshot>, AiaStoreError> {
    conn.query_row(
        "
        SELECT trace_id, request_kind, session_id, model, latest_started_at_ms, duration_ms,
               total_input_tokens, total_output_tokens, total_tokens, total_cached_tokens,
               estimated_cost_micros, lines_added, lines_removed,
               llm_span_count, tool_span_count, failed_tool_count, final_status
        FROM llm_trace_loops
        WHERE trace_id = ?1 OR id = ?1
        LIMIT 1
        ",
        [trace_id],
        |row| {
            Ok(LoopSummarySnapshot {
                trace_id: row.get(0)?,
                request_kind: row.get(1)?,
                session_id: row.get(2)?,
                model: row.get(3)?,
                latest_started_at_ms: row.get(4)?,
                duration_ms: row.get(5)?,
                total_input_tokens: row.get(6)?,
                total_output_tokens: row.get(7)?,
                total_tokens: row.get(8)?,
                total_cached_tokens: row.get(9)?,
                estimated_cost_micros: row.get(10)?,
                lines_added: row.get(11)?,
                lines_removed: row.get(12)?,
                llm_span_count: row.get(13)?,
                tool_span_count: row.get(14)?,
                failed_tool_count: row.get(15)?,
                final_status: LlmTraceLoopStatus::from_str(row.get::<_, String>(16)?.as_str()),
            })
        },
    )
    .optional()
    .map_err(AiaStoreError::from)
}

pub(super) fn refresh_summary_rollups_with_conn(
    conn: &Connection,
    change: &LoopSummaryChange,
) -> Result<(), AiaStoreError> {
    apply_summary_rollup_for_key_with_conn(
        conn,
        None,
        change.previous.as_ref(),
        change.current.as_ref(),
    )?;

    if let Some(previous) = change.previous.as_ref() {
        apply_summary_rollup_for_key_with_conn(
            conn,
            Some(previous.request_kind.as_str()),
            Some(previous),
            change.current.as_ref().filter(|current| current.request_kind == previous.request_kind),
        )?;
    }

    if let Some(current) = change.current.as_ref() {
        let current_request_kind = current.request_kind.as_str();
        let previous_for_current = change
            .previous
            .as_ref()
            .filter(|previous| previous.request_kind == current.request_kind);
        let previous_matches_current = previous_for_current.is_some();
        let previous_request_kind =
            change.previous.as_ref().map(|previous| previous.request_kind.as_str());
        if !previous_matches_current || previous_request_kind != Some(current_request_kind) {
            apply_summary_rollup_for_key_with_conn(
                conn,
                Some(current_request_kind),
                previous_for_current,
                Some(current),
            )?;
        }
    }

    Ok(())
}

fn apply_summary_rollup_for_key_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
    previous: Option<&LoopSummarySnapshot>,
    current: Option<&LoopSummarySnapshot>,
) -> Result<(), AiaStoreError> {
    if previous.is_none() && current.is_none() {
        return Ok(());
    }

    let request_kind_key = request_kind.unwrap_or("*");
    let mut row = load_summary_rollup_row_with_conn(conn, request_kind)?;

    row.summary.total_requests = apply_delta_u64(
        row.summary.total_requests,
        loop_presence_delta(previous, current),
        "total_requests",
    )?;
    row.summary.failed_requests = apply_delta_u64(
        row.summary.failed_requests,
        bool_metric_delta(previous.map(loop_failed_request), current.map(loop_failed_request)),
        "failed_requests",
    )?;
    row.summary.partial_requests = apply_delta_u64(
        row.summary.partial_requests,
        bool_metric_delta(previous.map(loop_partial_request), current.map(loop_partial_request)),
        "partial_requests",
    )?;
    row.summary.total_llm_spans = apply_delta_u64(
        row.summary.total_llm_spans,
        value_delta(
            previous.map(|loop_item| loop_item.llm_span_count),
            current.map(|loop_item| loop_item.llm_span_count),
        ),
        "total_llm_spans",
    )?;
    row.summary.total_tool_spans = apply_delta_u64(
        row.summary.total_tool_spans,
        value_delta(
            previous.map(|loop_item| loop_item.tool_span_count),
            current.map(|loop_item| loop_item.tool_span_count),
        ),
        "total_tool_spans",
    )?;
    row.summary.requests_with_tools = apply_delta_u64(
        row.summary.requests_with_tools,
        bool_metric_delta(previous.map(loop_has_tools), current.map(loop_has_tools)),
        "requests_with_tools",
    )?;
    row.summary.failed_tool_calls = apply_delta_u64(
        row.summary.failed_tool_calls,
        value_delta(
            previous.map(|loop_item| loop_item.failed_tool_count),
            current.map(|loop_item| loop_item.failed_tool_count),
        ),
        "failed_tool_calls",
    )?;
    row.summary.total_input_tokens = apply_delta_u64(
        row.summary.total_input_tokens,
        value_delta(
            previous.map(|loop_item| loop_item.total_input_tokens),
            current.map(|loop_item| loop_item.total_input_tokens),
        ),
        "total_input_tokens",
    )?;
    row.summary.total_output_tokens = apply_delta_u64(
        row.summary.total_output_tokens,
        value_delta(
            previous.map(|loop_item| loop_item.total_output_tokens),
            current.map(|loop_item| loop_item.total_output_tokens),
        ),
        "total_output_tokens",
    )?;
    row.summary.total_tokens = apply_delta_u64(
        row.summary.total_tokens,
        value_delta(
            previous.map(|loop_item| loop_item.total_tokens),
            current.map(|loop_item| loop_item.total_tokens),
        ),
        "total_tokens",
    )?;
    row.summary.total_cached_tokens = apply_delta_u64(
        row.summary.total_cached_tokens,
        value_delta(
            previous.map(|loop_item| loop_item.total_cached_tokens),
            current.map(|loop_item| loop_item.total_cached_tokens),
        ),
        "total_cached_tokens",
    )?;
    row.total_duration_ms = apply_delta_u64(
        row.total_duration_ms,
        value_delta(
            previous.and_then(|loop_item| loop_item.duration_ms),
            current.and_then(|loop_item| loop_item.duration_ms),
        ),
        "total_duration_ms",
    )?;
    row.duration_sample_count = apply_delta_u64(
        row.duration_sample_count,
        bool_metric_delta(
            previous.and_then(|loop_item| loop_item.duration_ms).map(|_| true),
            current.and_then(|loop_item| loop_item.duration_ms).map(|_| true),
        ),
        "duration_sample_count",
    )?;

    update_model_count_with_conn(
        conn,
        request_kind_key,
        previous.map(|loop_item| loop_item.model.as_str()),
        current.map(|loop_item| loop_item.model.as_str()),
    )?;
    update_duration_bucket_with_conn(
        conn,
        request_kind_key,
        previous.and_then(|loop_item| loop_item.duration_ms),
        current.and_then(|loop_item| loop_item.duration_ms),
    )?;

    row.summary.unique_models = count_models_for_summary_key_with_conn(conn, request_kind_key)?;
    row.summary.latest_request_started_at_ms = next_latest_started_at_ms_with_conn(
        conn,
        request_kind,
        row.summary.latest_request_started_at_ms,
        previous,
        current,
    )?;
    row.summary.avg_duration_ms = if row.duration_sample_count == 0 {
        None
    } else {
        Some(row.total_duration_ms as f64 / row.duration_sample_count as f64)
    };
    row.summary.p95_duration_ms = load_p95_duration_from_buckets_with_conn(
        conn,
        request_kind_key,
        row.duration_sample_count,
    )?;

    upsert_summary_rollup_row_with_conn(conn, request_kind_key, &row)
}

fn load_summary_rollup_row_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<SummaryRollupRow, AiaStoreError> {
    let request_kind_key = request_kind.unwrap_or("*");
    let cached = conn
        .query_row(
            "
            SELECT total_requests, failed_requests, partial_requests, avg_duration_ms,
                   p95_duration_ms, total_llm_spans, total_tool_spans, requests_with_tools,
                   failed_tool_calls, unique_models, latest_request_started_at_ms,
                   total_input_tokens, total_output_tokens, total_tokens, total_cached_tokens,
                   total_duration_ms, duration_sample_count
            FROM llm_trace_overview_summaries
            WHERE request_kind = ?1
            ",
            [request_kind_key],
            |row| {
                Ok(SummaryRollupRow {
                    summary: LlmTraceSummary {
                        total_requests: row.get(0)?,
                        failed_requests: row.get(1)?,
                        partial_requests: row.get(2)?,
                        avg_duration_ms: row.get(3)?,
                        p95_duration_ms: row.get(4)?,
                        total_llm_spans: row.get(5)?,
                        total_tool_spans: row.get(6)?,
                        requests_with_tools: row.get(7)?,
                        failed_tool_calls: row.get(8)?,
                        unique_models: row.get(9)?,
                        latest_request_started_at_ms: row.get(10)?,
                        total_input_tokens: row.get(11)?,
                        total_output_tokens: row.get(12)?,
                        total_tokens: row.get(13)?,
                        total_cached_tokens: row.get(14)?,
                    },
                    total_duration_ms: row.get(15)?,
                    duration_sample_count: row.get(16)?,
                })
            },
        )
        .optional()?;

    Ok(cached.unwrap_or_else(|| SummaryRollupRow {
        summary: LlmTraceSummary {
            total_requests: 0,
            failed_requests: 0,
            partial_requests: 0,
            avg_duration_ms: None,
            p95_duration_ms: None,
            total_llm_spans: 0,
            total_tool_spans: 0,
            requests_with_tools: 0,
            failed_tool_calls: 0,
            unique_models: 0,
            latest_request_started_at_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_tokens: 0,
            total_cached_tokens: 0,
        },
        total_duration_ms: 0,
        duration_sample_count: 0,
    }))
}

fn upsert_summary_rollup_row_with_conn(
    conn: &Connection,
    request_kind_key: &str,
    row: &SummaryRollupRow,
) -> Result<(), AiaStoreError> {
    let updated_at_ms = row.summary.latest_request_started_at_ms.unwrap_or(0);
    conn.execute(
        "
        INSERT INTO llm_trace_overview_summaries (
            request_kind, total_requests, failed_requests, partial_requests, avg_duration_ms,
            p95_duration_ms, total_llm_spans, total_tool_spans, requests_with_tools,
            failed_tool_calls, unique_models, latest_request_started_at_ms, total_input_tokens,
            total_output_tokens, total_tokens, total_cached_tokens, total_duration_ms,
            duration_sample_count, updated_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
        ON CONFLICT(request_kind) DO UPDATE SET
            total_requests = excluded.total_requests,
            failed_requests = excluded.failed_requests,
            partial_requests = excluded.partial_requests,
            avg_duration_ms = excluded.avg_duration_ms,
            p95_duration_ms = excluded.p95_duration_ms,
            total_llm_spans = excluded.total_llm_spans,
            total_tool_spans = excluded.total_tool_spans,
            requests_with_tools = excluded.requests_with_tools,
            failed_tool_calls = excluded.failed_tool_calls,
            unique_models = excluded.unique_models,
            latest_request_started_at_ms = excluded.latest_request_started_at_ms,
            total_input_tokens = excluded.total_input_tokens,
            total_output_tokens = excluded.total_output_tokens,
            total_tokens = excluded.total_tokens,
            total_cached_tokens = excluded.total_cached_tokens,
            total_duration_ms = excluded.total_duration_ms,
            duration_sample_count = excluded.duration_sample_count,
            updated_at_ms = excluded.updated_at_ms
        ",
        params![
            request_kind_key,
            row.summary.total_requests,
            row.summary.failed_requests,
            row.summary.partial_requests,
            row.summary.avg_duration_ms,
            row.summary.p95_duration_ms,
            row.summary.total_llm_spans,
            row.summary.total_tool_spans,
            row.summary.requests_with_tools,
            row.summary.failed_tool_calls,
            row.summary.unique_models,
            row.summary.latest_request_started_at_ms,
            row.summary.total_input_tokens,
            row.summary.total_output_tokens,
            row.summary.total_tokens,
            row.summary.total_cached_tokens,
            row.total_duration_ms,
            row.duration_sample_count,
            updated_at_ms,
        ],
    )?;
    Ok(())
}

fn update_model_count_with_conn(
    conn: &Connection,
    request_kind_key: &str,
    previous_model: Option<&str>,
    current_model: Option<&str>,
) -> Result<(), AiaStoreError> {
    if previous_model == current_model {
        return Ok(());
    }

    if let Some(model) = previous_model {
        adjust_model_count_with_conn(conn, request_kind_key, model, -1)?;
    }
    if let Some(model) = current_model {
        adjust_model_count_with_conn(conn, request_kind_key, model, 1)?;
    }

    Ok(())
}

fn adjust_model_count_with_conn(
    conn: &Connection,
    request_kind_key: &str,
    model: &str,
    delta: i64,
) -> Result<(), AiaStoreError> {
    let existing = conn
        .query_row(
            "
            SELECT loop_count
            FROM llm_trace_summary_model_counts
            WHERE request_kind = ?1 AND model = ?2
            ",
            params![request_kind_key, model],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let next = existing.unwrap_or(0) + delta;
    if next < 0 {
        return Err(AiaStoreError::new(format!(
            "summary model count underflow: {request_kind_key}/{model}"
        )));
    }
    if next == 0 {
        conn.execute(
            "DELETE FROM llm_trace_summary_model_counts WHERE request_kind = ?1 AND model = ?2",
            params![request_kind_key, model],
        )?;
        return Ok(());
    }

    conn.execute(
        "
        INSERT INTO llm_trace_summary_model_counts (request_kind, model, loop_count)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(request_kind, model) DO UPDATE SET loop_count = excluded.loop_count
        ",
        params![request_kind_key, model, next],
    )?;
    Ok(())
}

fn update_duration_bucket_with_conn(
    conn: &Connection,
    request_kind_key: &str,
    previous_duration: Option<u64>,
    current_duration: Option<u64>,
) -> Result<(), AiaStoreError> {
    if previous_duration == current_duration {
        return Ok(());
    }

    if let Some(duration_ms) = previous_duration {
        adjust_duration_bucket_with_conn(conn, request_kind_key, duration_ms, -1)?;
    }
    if let Some(duration_ms) = current_duration {
        adjust_duration_bucket_with_conn(conn, request_kind_key, duration_ms, 1)?;
    }

    Ok(())
}

fn adjust_duration_bucket_with_conn(
    conn: &Connection,
    request_kind_key: &str,
    duration_ms: u64,
    delta: i64,
) -> Result<(), AiaStoreError> {
    let existing = conn
        .query_row(
            "
            SELECT sample_count
            FROM llm_trace_summary_duration_buckets
            WHERE request_kind = ?1 AND duration_ms = ?2
            ",
            params![request_kind_key, duration_ms],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let next = existing.unwrap_or(0) + delta;
    if next < 0 {
        return Err(AiaStoreError::new(format!(
            "summary duration bucket underflow: {request_kind_key}/{duration_ms}"
        )));
    }
    if next == 0 {
        conn.execute(
            "DELETE FROM llm_trace_summary_duration_buckets WHERE request_kind = ?1 AND duration_ms = ?2",
            params![request_kind_key, duration_ms],
        )?;
        return Ok(());
    }

    conn.execute(
        "
        INSERT INTO llm_trace_summary_duration_buckets (request_kind, duration_ms, sample_count)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(request_kind, duration_ms) DO UPDATE SET sample_count = excluded.sample_count
        ",
        params![request_kind_key, duration_ms, next],
    )?;
    Ok(())
}

fn count_models_for_summary_key_with_conn(
    conn: &Connection,
    request_kind_key: &str,
) -> Result<u64, AiaStoreError> {
    conn.query_row(
        "
        SELECT COUNT(*)
        FROM llm_trace_summary_model_counts
        WHERE request_kind = ?1
        ",
        [request_kind_key],
        |row| row.get::<_, u64>(0),
    )
    .map_err(AiaStoreError::from)
}

fn next_latest_started_at_ms_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
    current_max: Option<u64>,
    previous: Option<&LoopSummarySnapshot>,
    current: Option<&LoopSummarySnapshot>,
) -> Result<Option<u64>, AiaStoreError> {
    let previous_latest = previous.map(|loop_item| loop_item.latest_started_at_ms);
    let current_latest = current.map(|loop_item| loop_item.latest_started_at_ms);
    let next_candidate = match (current_max, current_latest) {
        (Some(existing), Some(next)) => Some(existing.max(next)),
        (Some(existing), None) => Some(existing),
        (None, Some(next)) => Some(next),
        (None, None) => None,
    };

    let should_recalculate = match (current_max, previous_latest, current_latest) {
        (Some(max_value), Some(previous_value), Some(current_value)) => {
            previous_value == max_value && current_value < previous_value
        }
        (Some(max_value), Some(previous_value), None) => previous_value == max_value,
        _ => false,
    };

    if should_recalculate {
        return query_latest_started_at_ms_from_loops_with_conn(conn, request_kind);
    }

    Ok(next_candidate)
}

fn query_latest_started_at_ms_from_loops_with_conn(
    conn: &Connection,
    request_kind: Option<&str>,
) -> Result<Option<u64>, AiaStoreError> {
    if let Some(request_kind) = request_kind {
        conn.query_row(
            "SELECT MAX(latest_started_at_ms) FROM llm_trace_loops WHERE request_kind = ?1",
            [request_kind],
            |row| row.get::<_, Option<u64>>(0),
        )
        .map_err(AiaStoreError::from)
    } else {
        conn.query_row("SELECT MAX(latest_started_at_ms) FROM llm_trace_loops", [], |row| {
            row.get::<_, Option<u64>>(0)
        })
        .map_err(AiaStoreError::from)
    }
}

fn load_p95_duration_from_buckets_with_conn(
    conn: &Connection,
    request_kind_key: &str,
    total_samples: u64,
) -> Result<Option<u64>, AiaStoreError> {
    if total_samples == 0 {
        return Ok(None);
    }

    let target = ((total_samples as f64) * 0.95).ceil() as u64;
    let mut stmt = conn.prepare(
        "
        SELECT duration_ms, sample_count
        FROM llm_trace_summary_duration_buckets
        WHERE request_kind = ?1
        ORDER BY duration_ms ASC
        ",
    )?;
    let rows = stmt
        .query_map([request_kind_key], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)))?;

    let mut seen = 0_u64;
    for row in rows {
        let (duration_ms, sample_count) = row.map_err(AiaStoreError::from)?;
        seen = seen.saturating_add(sample_count);
        if seen >= target {
            return Ok(Some(duration_ms));
        }
    }

    Ok(None)
}

fn apply_delta_u64(current: u64, delta: i64, field: &str) -> Result<u64, AiaStoreError> {
    let next = i128::from(current) + i128::from(delta);
    if next < 0 {
        return Err(AiaStoreError::new(format!("summary field underflow: {field}")));
    }
    Ok(next as u64)
}

fn loop_presence_delta(
    previous: Option<&LoopSummarySnapshot>,
    current: Option<&LoopSummarySnapshot>,
) -> i64 {
    bool_to_i64(current.is_some()) - bool_to_i64(previous.is_some())
}

fn bool_metric_delta(previous: Option<bool>, current: Option<bool>) -> i64 {
    bool_to_i64(current.unwrap_or(false)) - bool_to_i64(previous.unwrap_or(false))
}

fn value_delta(previous: Option<u64>, current: Option<u64>) -> i64 {
    current.unwrap_or(0) as i64 - previous.unwrap_or(0) as i64
}

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn loop_failed_request(loop_item: &LoopSummarySnapshot) -> bool {
    loop_item.final_status == LlmTraceLoopStatus::Failed
}

fn loop_partial_request(loop_item: &LoopSummarySnapshot) -> bool {
    loop_item.final_status == LlmTraceLoopStatus::Partial
}

fn loop_has_tools(loop_item: &LoopSummarySnapshot) -> bool {
    loop_item.tool_span_count > 0
}

fn ensure_loop_rollup_invariants(
    trace_id: &str,
    llm_traces: &[LoopTraceRollupRow],
) -> Result<(), AiaStoreError> {
    let request_kinds = llm_traces
        .iter()
        .map(|trace| trace.request_kind.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if request_kinds.len() > 1 {
        return Err(AiaStoreError::new(format!(
            "trace loop contains mixed non-tool request kinds: {trace_id}"
        )));
    }

    let models = llm_traces
        .iter()
        .map(|trace| trace.model.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if models.len() > 1 {
        return Err(AiaStoreError::new(format!("trace loop contains mixed models: {trace_id}")));
    }

    Ok(())
}
