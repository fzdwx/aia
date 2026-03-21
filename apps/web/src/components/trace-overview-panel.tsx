import { useEffect, useMemo, useState } from "react"
import {
  ArrowRight,
  AlertTriangle,
  ArrowDownToLine,
  ArrowUpFromLine,
  DatabaseZap,
  FolderGit2,
  Layers3,
  Workflow,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type {
  TraceDashboard,
  TraceDashboardActivityPoint,
  TraceDashboardRange,
  TraceDashboardSummary,
  TraceSummary,
} from "@/lib/types"
import { useTraceOverviewStore } from "@/stores/trace-overview-store"
import { useTraceStore } from "@/stores/trace-store"

const compactNumberFormatter = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
})

const longDateFormatter = new Intl.DateTimeFormat("zh-CN", {
  hour12: false,
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
})

const shortDateFormatter = new Intl.DateTimeFormat("en-US", {
  month: "2-digit",
  day: "2-digit",
})

const hourFormatter = new Intl.DateTimeFormat("en-US", {
  hour: "2-digit",
})

const RANGE_OPTIONS: Array<{ value: TraceDashboardRange; label: string }> = [
  { value: "today", label: "Today" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
]

function formatCompactCount(value: number | null | undefined) {
  return value != null ? compactNumberFormatter.format(value) : "-"
}

function formatCount(value: number | null | undefined) {
  return value != null ? value.toLocaleString("en-US") : "-"
}

function formatPercent(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-"
  const percent = value * 100
  if (Math.abs(percent) >= 100) return `${Math.round(percent)}%`
  if (Math.abs(percent) >= 10) return `${percent.toFixed(0)}%`
  return `${percent.toFixed(1)}%`
}

function formatSignedCount(value: number) {
  return `${value >= 0 ? "+" : ""}${formatCount(Math.abs(value))}`
}

function formatActivityAt(value: number | null | undefined) {
  if (value == null) return "-"
  return longDateFormatter.format(new Date(value))
}

function tokenCacheRatio(summary: TraceDashboardSummary | null | undefined) {
  if (!summary || summary.total_input_tokens <= 0) return null
  return summary.total_cached_tokens / summary.total_input_tokens
}

function formatChartBucket(range: TraceDashboardRange, bucketStartMs: number) {
  const date = new Date(bucketStartMs)
  return range === "today"
    ? hourFormatter.format(date)
    : shortDateFormatter.format(date)
}

function totalCompleted(summary: TraceSummary | null | undefined) {
  if (!summary) return 0
  return Math.max(
    0,
    summary.total_requests - summary.failed_requests - summary.partial_requests
  )
}

function healthRate(summary: TraceSummary | null | undefined) {
  if (!summary || summary.total_requests <= 0) return null
  return totalCompleted(summary) / summary.total_requests
}

function buildLinePath(points: Array<{ x: number; y: number }>) {
  if (points.length === 0) return ""
  return points
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ")
}

function buildAreaPath(
  points: Array<{ x: number; y: number }>,
  height: number
) {
  if (points.length === 0) return ""
  const first = points[0]
  const last = points[points.length - 1]
  return `M ${first.x} ${height} L ${first.x} ${first.y} ${points
    .slice(1)
    .map((point) => `L ${point.x} ${point.y}`)
    .join(" ")} L ${last.x} ${height} Z`
}

function DashboardMetricCard({
  icon,
  label,
  value,
  detail,
}: {
  icon: React.ReactNode
  label: string
  value: string
  detail: string
}) {
  return (
    <section className="rounded-[22px] border border-border/35 bg-background/85 p-3 shadow-[0_18px_45px_-28px_rgba(15,23,42,0.32)]">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
            {label}
          </p>
          <p className="mt-2 text-[24px] font-semibold tracking-[-0.04em] text-foreground tabular-nums">
            {value}
          </p>
        </div>
        <div className="rounded-full border border-border/40 bg-muted/45 p-2 text-muted-foreground">
          {icon}
        </div>
      </div>
      <p className="mt-2 text-[11px] leading-4 text-muted-foreground">
        {detail}
      </p>
    </section>
  )
}

function TrendChartCard({ dashboard }: { dashboard: TraceDashboard }) {
  const [activeIndex, setActiveIndex] = useState<number | null>(null)

  const chart = useMemo(() => {
    const width = 760
    const height = 240
    const padding = { top: 16, right: 18, bottom: 28, left: 18 }
    const usableWidth = width - padding.left - padding.right
    const usableHeight = height - padding.top - padding.bottom
    const points = dashboard.trend
    const maxTokens = Math.max(
      1,
      ...points.flatMap((point) => [
        point.total_input_tokens,
        point.total_output_tokens,
        point.total_cached_tokens,
      ])
    )

    const toX = (index: number) =>
      padding.left +
      (points.length <= 1
        ? usableWidth / 2
        : (index / (points.length - 1)) * usableWidth)
    const toY = (value: number) =>
      padding.top + usableHeight - (value / maxTokens) * usableHeight

    const inputPoints = points.map((point, index) => ({
      x: toX(index),
      y: toY(point.total_input_tokens),
    }))
    const outputPoints = points.map((point, index) => ({
      x: toX(index),
      y: toY(point.total_output_tokens),
    }))
    const cachePoints = points.map((point, index) => ({
      x: toX(index),
      y: toY(point.total_cached_tokens),
    }))

    return {
      width,
      height,
      usableHeight,
      padding,
      inputAreaPath: buildAreaPath(inputPoints, padding.top + usableHeight),
      inputLinePath: buildLinePath(inputPoints),
      outputLinePath: buildLinePath(outputPoints),
      cacheLinePath: buildLinePath(cachePoints),
      inputPoints,
    }
  }, [dashboard])

  const resolvedIndex =
    activeIndex != null && activeIndex < dashboard.trend.length
      ? activeIndex
      : Math.max(0, dashboard.trend.length - 1)
  const activePoint = dashboard.trend[resolvedIndex]

  return (
    <section className="rounded-[28px] border border-border/35 bg-background/85 p-5 shadow-[0_20px_50px_-32px_rgba(15,23,42,0.35)]">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
            Token trend
          </p>
          <p className="mt-2 text-[24px] font-semibold tracking-[-0.04em] text-foreground">
            {formatCompactCount(dashboard.current.total_tokens)}
          </p>
          <p className="mt-1 text-[12px] text-muted-foreground">
            Input / output / cache across{" "}
            {formatCount(dashboard.current.total_requests)} loops in the
            selected range.
          </p>
        </div>

        <div className="min-w-[220px] rounded-[20px] border border-border/35 bg-muted/20 px-4 py-3">
          <p className="text-[11px] tracking-[0.14em] text-muted-foreground uppercase">
            {formatChartBucket(
              dashboard.range,
              activePoint?.bucket_start_ms ?? 0
            )}
          </p>
          <p className="mt-2 text-[20px] font-semibold tracking-[-0.03em] text-foreground tabular-nums">
            {formatCompactCount(activePoint?.total_tokens ?? 0)}
          </p>
          <div className="mt-3 space-y-1.5 text-[12px]">
            <div className="flex items-center justify-between gap-3">
              <span className="flex items-center gap-2 text-muted-foreground">
                <span
                  className="size-2 rounded-full"
                  style={{ backgroundColor: "var(--trace-chart-input)" }}
                />
                Input
              </span>
              <span className="font-medium text-foreground tabular-nums">
                {formatCompactCount(activePoint?.total_input_tokens ?? 0)}
              </span>
            </div>
            <div className="flex items-center justify-between gap-3">
              <span className="flex items-center gap-2 text-muted-foreground">
                <span
                  className="size-2 rounded-full"
                  style={{ backgroundColor: "var(--trace-chart-output)" }}
                />
                Output
              </span>
              <span className="font-medium text-foreground tabular-nums">
                {formatCompactCount(activePoint?.total_output_tokens ?? 0)}
              </span>
            </div>
            <div className="flex items-center justify-between gap-3">
              <span className="flex items-center gap-2 text-muted-foreground">
                <span
                  className="size-2 rounded-full"
                  style={{ backgroundColor: "var(--trace-chart-cache)" }}
                />
                Cache
              </span>
              <span className="font-medium text-foreground tabular-nums">
                {formatCompactCount(activePoint?.total_cached_tokens ?? 0)}
              </span>
            </div>
            <div className="flex items-center justify-between gap-3 border-t border-border/25 pt-2">
              <span className="text-muted-foreground">Failures</span>
              <span className="font-medium text-foreground tabular-nums">
                {formatCount(
                  (activePoint?.failed_requests ?? 0) +
                    (activePoint?.partial_requests ?? 0)
                )}
              </span>
            </div>
          </div>
        </div>
      </div>

      <div className="relative mt-5 overflow-hidden rounded-[22px] border border-border/30 bg-muted/[0.12] px-3 py-4">
        <svg
          viewBox={`0 0 ${chart.width} ${chart.height}`}
          className="h-[280px] w-full"
        >
          {Array.from({ length: 5 }).map((_, index) => {
            const y = chart.padding.top + (index / 4) * chart.usableHeight
            return (
              <line
                key={y}
                x1={chart.padding.left}
                x2={chart.width - chart.padding.right}
                y1={y}
                y2={y}
                stroke="currentColor"
                strokeWidth="1"
                className="text-border/35"
              />
            )
          })}

          <path d={chart.inputAreaPath} fill="url(#trace-token-area)" />
          <path
            d={chart.inputLinePath}
            fill="none"
            stroke="var(--trace-chart-input)"
            strokeWidth="2.5"
            strokeLinecap="round"
          />
          <path
            d={chart.outputLinePath}
            fill="none"
            stroke="var(--trace-chart-output)"
            strokeWidth="2"
            strokeLinecap="round"
          />
          <path
            d={chart.cacheLinePath}
            fill="none"
            stroke="var(--trace-chart-cache)"
            strokeWidth="2"
            strokeLinecap="round"
          />

          {chart.inputPoints.map((point, index) => (
            <g key={dashboard.trend[index]?.bucket_start_ms ?? index}>
              <circle
                cx={point.x}
                cy={point.y}
                r={index === resolvedIndex ? 4.5 : 2.5}
                fill="var(--trace-chart-input)"
              />
              <rect
                x={point.x - 12}
                y={chart.padding.top}
                width="24"
                height={chart.usableHeight}
                fill="transparent"
                onMouseEnter={() => setActiveIndex(index)}
                onFocus={() => setActiveIndex(index)}
              />
            </g>
          ))}

          <defs>
            <linearGradient id="trace-token-area" x1="0" y1="0" x2="0" y2="1">
              <stop
                offset="0%"
                stopColor="color-mix(in oklab, var(--trace-chart-input) 24%, transparent)"
              />
              <stop
                offset="100%"
                stopColor="color-mix(in oklab, var(--trace-chart-input) 2%, transparent)"
              />
            </linearGradient>
          </defs>
        </svg>

        <div className="mt-3 flex flex-wrap items-center gap-4 text-[12px] text-muted-foreground">
          <span className="font-medium text-foreground">Series</span>
          <span className="flex items-center gap-2">
            <span
              className="size-2 rounded-full"
              style={{ backgroundColor: "var(--trace-chart-input)" }}
            />
            Input
          </span>
          <span className="flex items-center gap-2">
            <span
              className="size-2 rounded-full"
              style={{ backgroundColor: "var(--trace-chart-output)" }}
            />
            Output
          </span>
          <span className="flex items-center gap-2">
            <span
              className="size-2 rounded-full"
              style={{ backgroundColor: "var(--trace-chart-cache)" }}
            />
            Cache
          </span>
        </div>
      </div>
    </section>
  )
}

function ActivityHeatmapCard({
  activity,
}: {
  activity: TraceDashboardActivityPoint[]
}) {
  const maxRequests = Math.max(
    1,
    ...activity.map((point) => point.total_requests)
  )
  const maxSessions = Math.max(
    1,
    ...activity.map((point) => point.total_sessions)
  )
  const maxLinesChanged = Math.max(
    1,
    ...activity.map((point) => point.total_lines_changed)
  )
  const cells = activity.slice(-364)

  return (
    <section className="rounded-[28px] border border-border/35 bg-background/85 p-5 shadow-[0_20px_50px_-32px_rgba(15,23,42,0.35)]">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
            Activity
          </p>
          <p className="mt-2 text-[22px] font-semibold tracking-[-0.04em] text-foreground">
            {formatCount(
              cells.filter((point) => point.total_sessions > 0).length
            )}{" "}
            active days in the last year
          </p>
        </div>
      </div>

      <div className="mt-5 grid grid-flow-col grid-rows-7 gap-1 overflow-x-auto rounded-[22px] border border-border/25 bg-muted/[0.12] p-4">
        {cells.map((point) => {
          const intensity = Math.max(
            point.total_requests / maxRequests,
            point.total_sessions / maxSessions,
            point.total_lines_changed / maxLinesChanged
          )
          const tone =
            intensity <= 0
              ? "bg-muted/35"
              : intensity < 0.25
                ? "bg-emerald-200/60"
                : intensity < 0.5
                  ? "bg-emerald-300/80"
                  : intensity < 0.75
                    ? "bg-emerald-400/85"
                    : "bg-emerald-500"
          return (
            <div
              key={point.day_start_ms}
              title={`${shortDateFormatter.format(new Date(point.day_start_ms))} · ${point.total_requests} requests · ${point.total_sessions} sessions`}
              className={cn(
                "size-3 rounded-[4px] border border-border/20 transition-transform hover:scale-110",
                tone
              )}
            />
          )
        })}
      </div>
    </section>
  )
}

function WorkspaceEntryCard({
  title,
  summary,
  accent,
  onOpen,
}: {
  title: string
  summary: TraceSummary
  accent: "violet" | "amber"
  onOpen: () => void
}) {
  const accentClass =
    accent === "violet"
      ? "from-violet-500/[0.10] via-background to-background"
      : "from-amber-500/[0.10] via-background to-background"

  return (
    <section
      className={cn(
        "rounded-[28px] border border-border/35 bg-linear-to-br p-5",
        accentClass
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
            {title}
          </p>
          <h3 className="mt-2 text-[24px] font-semibold tracking-[-0.04em] text-foreground">
            {formatCount(summary.total_requests)} requests
          </h3>
        </div>

        <Button variant="outline" size="sm" onClick={onOpen}>
          Open explorer
          <ArrowRight className="size-3.5" />
        </Button>
      </div>

      <div className="mt-4 grid gap-4 border-t border-border/20 pt-3 sm:grid-cols-2 xl:grid-cols-5">
        <div>
          <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
            health
          </p>
          <p className="mt-2 text-[18px] font-semibold text-foreground tabular-nums">
            {formatPercent(healthRate(summary))}
          </p>
        </div>
        <div>
          <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
            latency
          </p>
          <p className="mt-2 text-[18px] font-semibold text-foreground tabular-nums">
            {summary.p95_duration_ms != null
              ? `${summary.p95_duration_ms} ms`
              : "-"}
          </p>
        </div>
        <div>
          <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
            tokens
          </p>
          <p className="mt-2 text-[18px] font-semibold text-foreground tabular-nums">
            {formatCompactCount(summary.total_tokens)}
          </p>
        </div>
        <div>
          <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
            tools
          </p>
          <p className="mt-2 text-[18px] font-semibold text-foreground tabular-nums">
            {formatCount(summary.total_tool_spans)}
          </p>
        </div>
        <div>
          <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
            last trace
          </p>
          <p className="mt-2 text-[18px] font-semibold text-foreground tabular-nums">
            {formatActivityAt(summary.latest_request_started_at_ms)}
          </p>
        </div>
      </div>
    </section>
  )
}

export function TraceOverviewPanel() {
  const dashboard = useTraceOverviewStore((state) => state.dashboard)
  const range = useTraceOverviewStore((state) => state.range)
  const loading = useTraceOverviewStore((state) => state.loading)
  const error = useTraceOverviewStore((state) => state.error)
  const initialize = useTraceOverviewStore((state) => state.initialize)
  const setRange = useTraceOverviewStore((state) => state.setRange)
  const openWorkspace = useTraceStore((state) => state.openWorkspace)

  useEffect(() => {
    void initialize().catch(() => {})
  }, [initialize])

  const current = dashboard?.current ?? null
  const previous = dashboard?.previous ?? null

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-5 py-4">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[1440px] flex-col gap-4">
          <section className="flex flex-wrap items-center justify-between gap-3 rounded-[24px] border border-border/35 bg-background/80 px-4 py-3 shadow-[0_18px_45px_-32px_rgba(15,23,42,0.28)]">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-3 text-[12px] text-muted-foreground">
                <span className="font-medium text-foreground">Overview</span>
                <span>
                  Last trace{" "}
                  {formatActivityAt(
                    dashboard?.overall_summary.latest_request_started_at_ms
                  )}
                </span>
                <span>
                  P95{" "}
                  {dashboard?.overall_summary.p95_duration_ms != null
                    ? `${dashboard.overall_summary.p95_duration_ms} ms`
                    : "-"}
                </span>
                <span>
                  {formatCount(
                    dashboard?.overall_summary.total_tool_spans ?? 0
                  )}{" "}
                  tool spans
                </span>
              </div>
            </div>

            <div className="inline-flex rounded-full border border-border/40 bg-background/85 p-1">
              {RANGE_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => void setRange(option.value).catch(() => {})}
                  className={cn(
                    "rounded-full px-3 py-1.5 text-[12px] transition-colors",
                    option.value === range
                      ? "bg-foreground text-background"
                      : "text-muted-foreground hover:text-foreground"
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </section>

          {error ? (
            <div className="rounded-2xl border border-destructive/20 bg-destructive/[0.05] px-4 py-3 text-[12px] text-destructive">
              {error}
            </div>
          ) : null}

          <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-7">
            <DashboardMetricCard
              icon={<Workflow className="size-4" />}
              label="Requests"
              value={formatCount(current?.total_requests)}
              detail={
                current && previous
                  ? `${formatSignedCount(current.total_requests - previous.total_requests)} vs last range`
                  : "Loop requests in range"
              }
            />
            <DashboardMetricCard
              icon={<AlertTriangle className="size-4" />}
              label="Failures"
              value={formatCount(
                (current?.failed_requests ?? 0) +
                  (current?.partial_requests ?? 0)
              )}
              detail={
                current
                  ? `${formatCount(current.failed_requests)} failed · ${formatCount(current.partial_requests)} partial`
                  : "-"
              }
            />
            <DashboardMetricCard
              icon={<ArrowDownToLine className="size-4" />}
              label="Input"
              value={formatCompactCount(current?.total_input_tokens)}
              detail={
                current && current.total_requests > 0
                  ? `${formatCompactCount(Math.round(current.total_input_tokens / current.total_requests))} / req`
                  : "-"
              }
            />
            <DashboardMetricCard
              icon={<ArrowUpFromLine className="size-4" />}
              label="Output"
              value={formatCompactCount(current?.total_output_tokens)}
              detail={
                current && current.total_requests > 0
                  ? `${formatCompactCount(Math.round(current.total_output_tokens / current.total_requests))} / req`
                  : "-"
              }
            />
            <DashboardMetricCard
              icon={<DatabaseZap className="size-4" />}
              label="Cache"
              value={formatCompactCount(current?.total_cached_tokens)}
              detail={`${formatPercent(tokenCacheRatio(current))} input reused`}
            />
            <DashboardMetricCard
              icon={<FolderGit2 className="size-4" />}
              label="Changes"
              value={formatCompactCount(current?.total_lines_changed)}
              detail={
                current
                  ? `${formatCount(current.total_sessions)} sessions · +${formatCount(current.total_lines_added)} / -${formatCount(current.total_lines_removed)}`
                  : "-"
              }
            />
            <DashboardMetricCard
              icon={<Layers3 className="size-4" />}
              label="Sessions"
              value={formatCount(current?.total_sessions)}
              detail={
                current && previous
                  ? `${formatSignedCount(current.total_sessions - previous.total_sessions)} vs last range`
                  : "Distinct active sessions in range"
              }
            />
          </section>

          {dashboard ? <TrendChartCard dashboard={dashboard} /> : null}
          {dashboard ? (
            <ActivityHeatmapCard activity={dashboard.activity} />
          ) : null}

          {dashboard ? (
            <section className="grid gap-4 xl:grid-cols-2">
              <WorkspaceEntryCard
                title="Conversation"
                summary={dashboard.conversation_summary}
                accent="violet"
                onOpen={() => void openWorkspace("conversation")}
              />
              <WorkspaceEntryCard
                title="Compression"
                summary={dashboard.compression_summary}
                accent="amber"
                onOpen={() => void openWorkspace("compression")}
              />
            </section>
          ) : null}

          {loading && !dashboard ? (
            <div className="rounded-2xl border border-border/30 bg-background/65 px-4 py-6 text-[12px] text-muted-foreground">
              Loading dashboard analytics...
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}
