use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension};

use crate::{AiaStore, AiaStoreError};

use super::{
    LlmTraceDashboard, LlmTraceDashboardActivityPoint, LlmTraceDashboardRange,
    LlmTraceDashboardSummary, LlmTraceDashboardTrendPoint,
    store::{
        enqueue_legacy_trace_loop_backfill_with_conn, load_summary_snapshot_with_conn,
        reconcile_dirty_trace_loops_with_conn, with_write_transaction,
    },
};

const HOUR_MS: u64 = 60 * 60 * 1000;
const DAY_MS: u64 = 24 * HOUR_MS;
const YEAR_DAYS: u64 = 365;

impl AiaStore {
    pub fn trace_dashboard(
        &self,
        range: LlmTraceDashboardRange,
    ) -> Result<LlmTraceDashboard, AiaStoreError> {
        self.with_conn(|conn| load_trace_dashboard_with_conn(conn, range))
    }

    pub async fn trace_dashboard_async(
        self: &Arc<Self>,
        range: LlmTraceDashboardRange,
    ) -> Result<LlmTraceDashboard, AiaStoreError> {
        self.with_conn_async(move |conn| load_trace_dashboard_with_conn(conn, range)).await
    }
}

fn load_trace_dashboard_with_conn(
    conn: &Connection,
    range: LlmTraceDashboardRange,
) -> Result<LlmTraceDashboard, AiaStoreError> {
    with_write_transaction(conn, |conn| {
        enqueue_legacy_trace_loop_backfill_with_conn(conn)?;
        reconcile_dirty_trace_loops_with_conn(conn)
    })?;

    let now_ms = current_timestamp_ms();
    let current_window = dashboard_window(range, now_ms);
    let previous_window = DashboardWindow {
        start_ms: current_window.start_ms.saturating_sub(current_window.span_ms),
        end_ms: current_window.start_ms,
        bucket_ms: current_window.bucket_ms,
        span_ms: current_window.span_ms,
    };

    Ok(LlmTraceDashboard {
        range,
        current: load_dashboard_summary_with_conn(conn, &current_window)?,
        previous: load_dashboard_summary_with_conn(conn, &previous_window)?,
        trend: load_dashboard_trend_with_conn(conn, &current_window)?,
        activity: load_dashboard_activity_with_conn(conn, now_ms)?,
        overall_summary: load_summary_snapshot_with_conn(conn, None)?,
        conversation_summary: load_summary_snapshot_with_conn(conn, Some("completion"))?,
        compression_summary: load_summary_snapshot_with_conn(conn, Some("compression"))?,
    })
}

#[derive(Clone, Copy)]
struct DashboardWindow {
    start_ms: u64,
    end_ms: u64,
    bucket_ms: u64,
    span_ms: u64,
}

fn dashboard_window(range: LlmTraceDashboardRange, now_ms: u64) -> DashboardWindow {
    match range {
        LlmTraceDashboardRange::Today => DashboardWindow {
            start_ms: align_timestamp(now_ms.saturating_sub(DAY_MS), HOUR_MS),
            end_ms: now_ms,
            bucket_ms: HOUR_MS,
            span_ms: DAY_MS,
        },
        LlmTraceDashboardRange::Week => DashboardWindow {
            start_ms: align_timestamp(now_ms.saturating_sub(7 * DAY_MS), DAY_MS),
            end_ms: now_ms,
            bucket_ms: DAY_MS,
            span_ms: 7 * DAY_MS,
        },
        LlmTraceDashboardRange::Month => DashboardWindow {
            start_ms: align_timestamp(now_ms.saturating_sub(30 * DAY_MS), DAY_MS),
            end_ms: now_ms,
            bucket_ms: DAY_MS,
            span_ms: 30 * DAY_MS,
        },
    }
}

fn load_dashboard_summary_with_conn(
    conn: &Connection,
    window: &DashboardWindow,
) -> Result<LlmTraceDashboardSummary, AiaStoreError> {
    let summary = conn
        .query_row(
            "
            SELECT COUNT(*),
                   COALESCE(SUM(CASE WHEN final_status = 'failed' THEN 1 ELSE 0 END), 0),
                   COALESCE(SUM(CASE WHEN final_status = 'partial' THEN 1 ELSE 0 END), 0),
                   COUNT(DISTINCT session_id),
                   COALESCE(SUM(total_input_tokens), 0),
                   COALESCE(SUM(total_output_tokens), 0),
                   COALESCE(SUM(total_tokens), 0),
                   COALESCE(SUM(total_cached_tokens), 0),
                   COALESCE(SUM(estimated_cost_micros), 0),
                   COALESCE(SUM(lines_added), 0),
                   COALESCE(SUM(lines_removed), 0)
            FROM llm_trace_loops
            WHERE latest_started_at_ms >= ?1 AND latest_started_at_ms < ?2
            ",
            [window.start_ms, window.end_ms],
            |row| {
                let total_lines_added = row.get::<_, u64>(9)?;
                let total_lines_removed = row.get::<_, u64>(10)?;
                Ok(LlmTraceDashboardSummary {
                    total_cost_usd: micros_to_usd(row.get::<_, u64>(8)?),
                    total_requests: row.get(0)?,
                    failed_requests: row.get(1)?,
                    partial_requests: row.get(2)?,
                    total_sessions: row.get(3)?,
                    total_input_tokens: row.get(4)?,
                    total_output_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                    total_cached_tokens: row.get(7)?,
                    total_lines_added,
                    total_lines_removed,
                    total_lines_changed: total_lines_added.saturating_add(total_lines_removed),
                })
            },
        )
        .optional()?;

    Ok(summary.unwrap_or(LlmTraceDashboardSummary {
        total_cost_usd: 0.0,
        total_requests: 0,
        failed_requests: 0,
        partial_requests: 0,
        total_sessions: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_tokens: 0,
        total_cached_tokens: 0,
        total_lines_added: 0,
        total_lines_removed: 0,
        total_lines_changed: 0,
    }))
}

fn load_dashboard_trend_with_conn(
    conn: &Connection,
    window: &DashboardWindow,
) -> Result<Vec<LlmTraceDashboardTrendPoint>, AiaStoreError> {
    let mut stmt = conn.prepare(
        "
        SELECT ((latest_started_at_ms / ?1) * ?1) AS bucket_start_ms,
               COUNT(*) AS total_requests,
               COALESCE(SUM(CASE WHEN final_status = 'failed' THEN 1 ELSE 0 END), 0) AS failed_requests,
               COALESCE(SUM(CASE WHEN final_status = 'partial' THEN 1 ELSE 0 END), 0) AS partial_requests,
               COALESCE(SUM(total_input_tokens), 0) AS total_input_tokens,
               COALESCE(SUM(total_output_tokens), 0) AS total_output_tokens,
               COALESCE(SUM(total_cached_tokens), 0) AS total_cached_tokens,
               COALESCE(SUM(total_tokens), 0) AS total_tokens
        FROM llm_trace_loops
        WHERE latest_started_at_ms >= ?2 AND latest_started_at_ms < ?3
        GROUP BY bucket_start_ms
        ORDER BY bucket_start_ms ASC
        ",
    )?;

    let rows = stmt
        .query_map([window.bucket_ms, window.start_ms, window.end_ms], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, u64>(5)?,
                row.get::<_, u64>(6)?,
                row.get::<_, u64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    let mut by_bucket = BTreeMap::<u64, LlmTraceDashboardTrendPoint>::new();
    for (
        bucket_start_ms,
        total_requests,
        failed_requests,
        partial_requests,
        total_input_tokens,
        total_output_tokens,
        total_cached_tokens,
        total_tokens,
    ) in rows
    {
        let entry = by_bucket.entry(bucket_start_ms).or_insert(LlmTraceDashboardTrendPoint {
            bucket_start_ms,
            total_requests: 0,
            failed_requests: 0,
            partial_requests: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cached_tokens: 0,
            total_tokens: 0,
        });
        entry.total_requests = entry.total_requests.saturating_add(total_requests);
        entry.failed_requests = entry.failed_requests.saturating_add(failed_requests);
        entry.partial_requests = entry.partial_requests.saturating_add(partial_requests);
        entry.total_input_tokens = entry.total_input_tokens.saturating_add(total_input_tokens);
        entry.total_output_tokens = entry.total_output_tokens.saturating_add(total_output_tokens);
        entry.total_cached_tokens = entry.total_cached_tokens.saturating_add(total_cached_tokens);
        entry.total_tokens = entry.total_tokens.saturating_add(total_tokens);
    }

    let mut trend = Vec::new();
    let mut bucket = window.start_ms;
    while bucket < window.end_ms {
        trend.push(by_bucket.remove(&bucket).unwrap_or(LlmTraceDashboardTrendPoint {
            bucket_start_ms: bucket,
            total_requests: 0,
            failed_requests: 0,
            partial_requests: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cached_tokens: 0,
            total_tokens: 0,
        }));
        bucket = bucket.saturating_add(window.bucket_ms);
    }

    Ok(trend)
}

fn load_dashboard_activity_with_conn(
    conn: &Connection,
    now_ms: u64,
) -> Result<Vec<LlmTraceDashboardActivityPoint>, AiaStoreError> {
    let start_ms = align_timestamp(now_ms.saturating_sub(YEAR_DAYS * DAY_MS), DAY_MS);
    let end_ms = now_ms.saturating_add(DAY_MS);
    let mut stmt = conn.prepare(
        "
        SELECT ((latest_started_at_ms / ?1) * ?1) AS day_start_ms,
               COUNT(*) AS total_requests,
               COUNT(DISTINCT session_id) AS total_sessions,
               COALESCE(SUM(estimated_cost_micros), 0) AS total_cost_micros,
               COALESCE(SUM(total_tokens), 0) AS total_tokens,
               COALESCE(SUM(lines_added + lines_removed), 0) AS total_lines_changed
        FROM llm_trace_loops
        WHERE latest_started_at_ms >= ?2 AND latest_started_at_ms < ?3
        GROUP BY day_start_ms
        ORDER BY day_start_ms ASC
        ",
    )?;

    let rows = stmt
        .query_map([DAY_MS, start_ms, end_ms], |row| {
            Ok(LlmTraceDashboardActivityPoint {
                day_start_ms: row.get(0)?,
                total_requests: row.get(1)?,
                total_sessions: row.get(2)?,
                total_cost_usd: micros_to_usd(row.get::<_, u64>(3)?),
                total_tokens: row.get(4)?,
                total_lines_changed: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AiaStoreError::from)?;

    let mut by_day =
        rows.into_iter().map(|point| (point.day_start_ms, point)).collect::<BTreeMap<_, _>>();

    let mut activity = Vec::new();
    let mut day = start_ms;
    while day < end_ms {
        activity.push(by_day.remove(&day).unwrap_or(LlmTraceDashboardActivityPoint {
            day_start_ms: day,
            total_requests: 0,
            total_sessions: 0,
            total_cost_usd: 0.0,
            total_tokens: 0,
            total_lines_changed: 0,
        }));
        day = day.saturating_add(DAY_MS);
    }

    Ok(activity)
}

fn align_timestamp(value: u64, bucket_ms: u64) -> u64 {
    (value / bucket_ms) * bucket_ms
}

fn micros_to_usd(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
