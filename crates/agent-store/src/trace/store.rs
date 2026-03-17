use rusqlite::{OptionalExtension, params};

use crate::{AiaStore, AiaStoreError};

use super::{
    LlmTraceListItem, LlmTraceListPage, LlmTraceRecord, LlmTraceStore, LlmTraceSummary,
    mapping::{read_trace_list_item, read_trace_record},
};

impl LlmTraceStore for AiaStore {
    fn record(&self, record: &LlmTraceRecord) -> Result<(), AiaStoreError> {
        self.lock_conn().execute(
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
                record.cached_tokens.map(|value| value as i64),
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
        let conn = self.lock_conn();
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
                   t.total_tokens, t.cached_tokens, t.provider_request, t.error
            FROM llm_request_traces t
            JOIN paged_loops p ON p.trace_id = t.trace_id
            ORDER BY p.latest_started_at_ms DESC, t.started_at_ms DESC, t.id DESC
            ",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], read_trace_list_item)?;

        Ok(LlmTraceListPage {
            items: rows.collect::<Result<Vec<_>, _>>().map_err(AiaStoreError::from)?,
            total_loops,
            page: offset / limit + 1,
            page_size: limit,
        })
    }

    fn get(&self, id: &str) -> Result<Option<LlmTraceRecord>, AiaStoreError> {
        let conn = self.lock_conn();
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

    fn summary(&self) -> Result<LlmTraceSummary, AiaStoreError> {
        let conn = self.lock_conn();
        let (
            total_requests,
            failed_requests,
            total_input_tokens,
            total_output_tokens,
            total_tokens,
            total_cached_tokens,
        ): (u64, u64, u64, u64, u64, u64) = conn.query_row(
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
        )?;

        let durations = load_client_durations(&conn)?;
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
}

fn load_client_durations(conn: &rusqlite::Connection) -> Result<Vec<u64>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "SELECT duration_ms FROM llm_request_traces WHERE span_kind = 'CLIENT' AND duration_ms IS NOT NULL ORDER BY duration_ms ASC",
    )?;
    stmt.query_map([], |row| row.get::<_, u64>(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)
}
