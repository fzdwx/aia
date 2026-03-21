import { useEffect, useMemo, useState, type ReactNode } from "react"
import { AlertTriangle, ArrowRight, DatabaseZap, Workflow } from "lucide-react"

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

const shortDateFormatter = new Intl.DateTimeFormat("en-US", {
  month: "2-digit",
  day: "2-digit",
})

const monthFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  timeZone: "UTC",
})

const monthDayFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  timeZone: "UTC",
})

const hourFormatter = new Intl.DateTimeFormat("en-US", {
  hour: "2-digit",
})

const DAY_MS = 24 * 60 * 60 * 1000

const HEATMAP_WEEKDAY_LABELS = [
  { row: 1, label: "Mon" },
  { row: 3, label: "Wed" },
  { row: 5, label: "Fri" },
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

function formatUsd(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-"
  if (value >= 1) return `$${value.toFixed(2)}`
  if (value >= 0.01) return `$${value.toFixed(3)}`
  return `$${value.toFixed(4)}`
}

function formatSignedCount(value: number) {
  return `${value >= 0 ? "+" : ""}${formatCount(Math.abs(value))}`
}

function tokenCacheRatio(summary: TraceDashboardSummary | null | undefined) {
  if (!summary || summary.total_input_tokens <= 0) return null
  return summary.total_cached_tokens / summary.total_input_tokens
}

function netInputTokens(
  inputTokens: number | null | undefined,
  cachedTokens: number | null | undefined
) {
  return Math.max(0, (inputTokens ?? 0) - (cachedTokens ?? 0))
}

function formatChartBucket(range: TraceDashboardRange, bucketStartMs: number) {
  const date = new Date(bucketStartMs)
  return range === "today"
    ? hourFormatter.format(date)
    : shortDateFormatter.format(date)
}

function formatLatency(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-"
  if (value < 1_000) return `${Math.round(value)} ms`

  const totalSeconds = Math.round(value / 1_000)
  if (totalSeconds < 60) {
    const seconds = value / 1_000
    return `${seconds >= 10 ? seconds.toFixed(0) : seconds.toFixed(1)} s`
  }

  const hours = Math.floor(totalSeconds / 3_600)
  const minutes = Math.floor((totalSeconds % 3_600) / 60)
  const seconds = totalSeconds % 60

  if (hours > 0) {
    return `${hours}h ${String(minutes).padStart(2, "0")}m`
  }

  return `${minutes}m ${String(seconds).padStart(2, "0")}s`
}

function totalCompleted(
  summary:
    | Pick<
        TraceSummary | TraceDashboardSummary,
        "total_requests" | "failed_requests" | "partial_requests"
      >
    | null
    | undefined
) {
  if (!summary) return 0
  return Math.max(
    0,
    summary.total_requests - summary.failed_requests - summary.partial_requests
  )
}

function healthRate(
  summary:
    | Pick<
        TraceSummary | TraceDashboardSummary,
        "total_requests" | "failed_requests" | "partial_requests"
      >
    | null
    | undefined
) {
  if (!summary || summary.total_requests <= 0) return null
  return totalCompleted(summary) / summary.total_requests
}

function toolUsageRate(summary: TraceSummary | null | undefined) {
  if (!summary || summary.total_requests <= 0) return null
  return summary.requests_with_tools / summary.total_requests
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

function SeriesMarker({
  color,
  shape,
}: {
  color: string
  shape: "circle" | "square" | "diamond"
}) {
  if (shape === "diamond") {
    return (
      <span className="inline-flex size-2 items-center justify-center">
        <span
          className="block size-1.5 rotate-45 rounded-[1px]"
          style={{ backgroundColor: color }}
        />
      </span>
    )
  }

  return (
    <span
      className={cn("size-2", shape === "circle" ? "rounded-full" : "rounded")}
      style={{ backgroundColor: color }}
    />
  )
}

function issueCount(
  summary:
    | Pick<
        TraceSummary | TraceDashboardSummary,
        "failed_requests" | "partial_requests"
      >
    | null
    | undefined
) {
  if (!summary) return 0
  return summary.failed_requests + summary.partial_requests
}

function issueRate(
  summary:
    | Pick<
        TraceSummary | TraceDashboardSummary,
        "total_requests" | "failed_requests" | "partial_requests"
      >
    | null
    | undefined
) {
  if (!summary || summary.total_requests <= 0) return null
  return issueCount(summary) / summary.total_requests
}

function OverviewSignalStat({
  label,
  value,
  detail,
  tone = "default",
}: {
  label: string
  value: string
  detail: string
  tone?: "default" | "alert"
}) {
  return (
    <div
      className={cn(
        "min-w-0 rounded-[16px] border px-3 py-2.5",
        tone === "alert"
          ? "border-destructive/28 bg-destructive/[0.06]"
          : "border-border/16 bg-muted/[0.04]"
      )}
    >
      <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
        {label}
      </p>
      <p className="mt-1 text-[16px] font-semibold tracking-[-0.03em] text-foreground tabular-nums">
        {value}
      </p>
      <p className="mt-1 truncate text-[10px] text-muted-foreground">
        {detail}
      </p>
    </div>
  )
}

function WorkspaceDrillButton({
  title,
  summary,
  detail,
  icon,
  onOpen,
}: {
  title: string
  summary: TraceSummary
  detail: string
  icon: ReactNode
  onOpen: () => void
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="trace-overview-drill-button w-full rounded-[14px] border border-border/18 bg-background/70 px-2.5 py-2 text-left transition-colors"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[11px] font-medium text-foreground">{title}</p>
          <p className="mt-0.5 text-[10px] text-muted-foreground">{detail}</p>
        </div>
        <span className="rounded-full border border-border/24 bg-background/70 p-1.5 text-muted-foreground">
          {icon}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground">
        <span>{formatCount(summary.total_requests)} req</span>
        <span>·</span>
        <span>{formatPercent(healthRate(summary))} healthy</span>
        <span>·</span>
        <span>{formatLatency(summary.p95_duration_ms)} p95</span>
        <span className="inline-flex items-center gap-1 text-foreground">
          Open workspace
          <ArrowRight className="size-3" />
        </span>
      </div>
    </button>
  )
}

function OverviewHeroCard({
  dashboard,
  onOpenConversation,
  onOpenCompression,
}: {
  dashboard: TraceDashboard
  onOpenConversation: () => void
  onOpenCompression: () => void
}) {
  const requestDelta =
    dashboard.current.total_requests - dashboard.previous.total_requests
  const healthyCount = totalCompleted(dashboard.current)
  const issueDelta =
    issueCount(dashboard.current) - issueCount(dashboard.previous)

  return (
    <section className="trace-overview-card trace-overview-section trace-overview-primary rounded-[20px] border border-border/25 bg-card/95 p-3 md:p-4">
      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_340px] xl:items-stretch">
        <div className="space-y-2">
          <div className="rounded-[14px] border border-border/16 bg-background/55 px-2.5 py-2">
            <ActivityHeatmapBlock activity={dashboard.activity} compact />
          </div>

          <div className="grid gap-2 md:grid-cols-3">
            <OverviewSignalStat
              label="Requests"
              value={formatCount(dashboard.current.total_requests)}
              detail={`${formatSignedCount(requestDelta)} vs previous range`}
            />
            <OverviewSignalStat
              label="Healthy completions"
              value={formatPercent(healthRate(dashboard.current))}
              detail={`${formatCount(healthyCount)} completed · ${formatCount(dashboard.current.total_sessions)} sessions`}
            />
            <OverviewSignalStat
              label="Active issues"
              value={formatCount(issueCount(dashboard.current))}
              detail={`${formatSignedCount(issueDelta)} vs previous · ${formatLatency(dashboard.overall_summary.p95_duration_ms)} p95`}
              tone={issueCount(dashboard.current) > 0 ? "alert" : "default"}
            />
          </div>
        </div>

        <section className="trace-overview-drill rounded-[16px] border border-border/20 bg-muted/[0.045] px-3 py-2.5">
          <div className="flex items-center justify-between gap-2">
            <p className="text-[11px] tracking-[0.14em] text-muted-foreground uppercase">
              Drill into workspace
            </p>
            <span className="text-[11px] text-muted-foreground">
              Primary tasks
            </span>
          </div>
          <p className="mt-1 text-[10px] leading-4 text-muted-foreground">
            Pick a slice for span-level inspection.
          </p>
          <div className="mt-2 space-y-2">
            <WorkspaceDrillButton
              title="Conversation workspace"
              summary={dashboard.conversation_summary}
              detail="Loop and tool chain diagnosis"
              icon={<Workflow className="size-3.5" />}
              onOpen={onOpenConversation}
            />
            <WorkspaceDrillButton
              title="Compression workspace"
              summary={dashboard.compression_summary}
              detail="Compaction timing and payload outcomes"
              icon={<DatabaseZap className="size-3.5" />}
              onOpen={onOpenCompression}
            />
          </div>
        </section>
      </div>
    </section>
  )
}

function TrendSignalBlock({
  label,
  value,
  detail,
  icon,
  metrics,
}: {
  label: string
  value: string
  detail: string
  icon?: ReactNode
  metrics?: Array<{ label: string; value: string }>
}) {
  return (
    <div className="min-w-0 border-l border-border/18 pl-3 first:border-l-0 first:pl-0">
      <p className="text-[10px] tracking-[0.14em] text-muted-foreground uppercase">
        {label}
      </p>
      <p className="mt-1.5 flex items-center gap-1.5 text-[14px] font-semibold text-foreground tabular-nums">
        {icon}
        {value}
      </p>
      <p className="mt-0.5 truncate text-[10px] text-muted-foreground">
        {detail}
      </p>
      {metrics && metrics.length > 0 ? (
        <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
          {metrics.map((metric) => (
            <span
              key={metric.label}
              className="inline-flex items-center gap-1 rounded-full border border-border/18 bg-background/55 px-1.5 py-0.5 text-[9px] text-muted-foreground"
            >
              <span className="uppercase">{metric.label}</span>
              <span className="font-medium text-foreground tabular-nums">
                {metric.value}
              </span>
            </span>
          ))}
        </div>
      ) : null}
    </div>
  )
}

function TrendChartBlock({ dashboard }: { dashboard: TraceDashboard }) {
  const [activeIndex, setActiveIndex] = useState<number | null>(null)

  const chart = useMemo(() => {
    const width = 760
    const height = 220
    const padding = { top: 14, right: 16, bottom: 22, left: 16 }
    const usableWidth = width - padding.left - padding.right
    const usableHeight = height - padding.top - padding.bottom
    const points = dashboard.trend
    const maxTokens = Math.max(
      1,
      ...points.flatMap((point) => [
        netInputTokens(point.total_input_tokens, point.total_cached_tokens),
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
      y: toY(
        netInputTokens(point.total_input_tokens, point.total_cached_tokens)
      ),
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
  const activeIssueRate = issueRate({
    total_requests: activePoint?.total_requests ?? 0,
    failed_requests: activePoint?.failed_requests ?? 0,
    partial_requests: activePoint?.partial_requests ?? 0,
  })
  const rangeIssueDelta =
    issueCount(dashboard.current) - issueCount(dashboard.previous)

  return (
    <section className="space-y-3">
      <div className="flex flex-col gap-2.5 xl:flex-row xl:items-start xl:justify-between">
        <div className="max-w-[760px]">
          <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
            Recent throughput & anomalies
          </p>
          <p className="mt-1.5 text-[20px] font-semibold tracking-[-0.04em] text-foreground tabular-nums">
            {formatCompactCount(dashboard.current.total_tokens)} tokens in range
          </p>
          <p className="mt-0.5 text-[10px] leading-4 text-muted-foreground">
            Prioritize the latest buckets before year-scale context. Net input
            removes cached tokens so trend changes map to fresh context demand.
          </p>
        </div>

        <div className="grid gap-1.5 rounded-[14px] border border-border/18 bg-background/65 px-2.5 py-1.5 sm:grid-cols-3 xl:min-w-[640px]">
          <TrendSignalBlock
            label="Active bucket"
            value={formatChartBucket(
              dashboard.range,
              activePoint?.bucket_start_ms ?? 0
            )}
            detail={`${formatCompactCount(activePoint?.total_tokens ?? 0)} tokens · ${formatCount(activePoint?.total_requests ?? 0)} requests`}
          />
          <TrendSignalBlock
            label="Anomaly watch"
            value={formatPercent(activeIssueRate)}
            detail={`${formatSignedCount(rangeIssueDelta)} range issues · ${formatCount(issueCount(dashboard.current))} total`}
            icon={<AlertTriangle className="size-3.5 text-destructive" />}
          />
          <TrendSignalBlock
            label="Execution mix"
            value={formatPercent(toolUsageRate(dashboard.overall_summary))}
            detail={`${formatCount(dashboard.overall_summary.requests_with_tools)} requests invoked tools`}
            metrics={[
              {
                label: "cache",
                value: formatPercent(tokenCacheRatio(dashboard.current)),
              },
              {
                label: "p95",
                value: formatLatency(dashboard.overall_summary.p95_duration_ms),
              },
              {
                label: "failed",
                value: formatCount(dashboard.overall_summary.failed_tool_calls),
              },
            ]}
          />
        </div>
      </div>

      <div className="trace-overview-chart relative overflow-hidden rounded-[18px] border border-border/18 bg-muted/[0.06] px-2.5 py-2.5">
        <svg
          viewBox={`0 0 ${chart.width} ${chart.height}`}
          className="h-[218px] w-full"
          preserveAspectRatio="none"
          aria-hidden="true"
          focusable="false"
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
            strokeDasharray="7 5"
          />
          <path
            d={chart.cacheLinePath}
            fill="none"
            stroke="var(--trace-chart-cache)"
            strokeWidth="2"
            strokeLinecap="round"
            strokeDasharray="2 6"
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

        <div className="mt-2 flex flex-wrap items-center gap-2.5 text-[11px] text-muted-foreground">
          <span className="font-medium text-foreground">Series</span>
          <span className="flex items-center gap-2">
            <SeriesMarker color="var(--trace-chart-input)" shape="circle" />
            Net input
          </span>
          <span className="flex items-center gap-2">
            <SeriesMarker color="var(--trace-chart-output)" shape="square" />
            Output
          </span>
          <span className="flex items-center gap-2">
            <SeriesMarker color="var(--trace-chart-cache)" shape="diamond" />
            Cache
          </span>
        </div>
      </div>
    </section>
  )
}

function ActivityHeatmapBlock({
  activity,
  compact = false,
}: {
  activity: TraceDashboardActivityPoint[]
  compact?: boolean
}) {
  const [activeDayStartMs, setActiveDayStartMs] = useState<number | null>(null)
  const cells = useMemo(() => activity.slice(-364), [activity])
  const activeDays = useMemo(
    () => cells.filter((point) => point.total_sessions > 0),
    [cells]
  )
  const recentActiveDays = useMemo(
    () => cells.slice(-30).filter((point) => point.total_sessions > 0).length,
    [cells]
  )
  const latestActiveDay = activeDays[activeDays.length - 1] ?? null
  const highlightedDay =
    cells.find((point) => point.day_start_ms === activeDayStartMs) ??
    latestActiveDay
  const heatmap = useMemo(() => {
    const cellSize = compact ? 8 : 10
    const columnGap = compact ? 5 : 4
    const rowGap = compact ? 2 : 4
    const leftAxisWidth = compact ? 22 : 30
    const topAxisHeight = compact ? 14 : 18
    const monthLabelY = compact ? 8 : 10
    const weekdayLabelOffset = compact ? 6 : 8
    const cellRadius = compact ? 2 : 3
    const trackedDays = 364

    if (cells.length === 0) {
      return {
        width: 0,
        height: 0,
        cellSize,
        columnGap,
        rowGap,
        cellRadius,
        leftAxisWidth,
        topAxisHeight,
        monthLabelY,
        weekdayLabelOffset,
        monthLabels: [] as Array<{ key: string; label: string; x: number }>,
        cells: [] as Array<{
          key: number
          point: TraceDashboardActivityPoint
          x: number
          y: number
          level: 0 | 1 | 2 | 3 | 4
        }>,
      }
    }

    const sortedCells = [...cells].sort(
      (left, right) => left.day_start_ms - right.day_start_ms
    )
    const pointsByDay = new Map(
      sortedCells.map((point) => [point.day_start_ms, point] as const)
    )
    const latestKnownDay =
      sortedCells[sortedCells.length - 1]?.day_start_ms ??
      Math.floor(Date.now() / DAY_MS) * DAY_MS
    const rangeStartMs = latestKnownDay - (trackedDays - 1) * DAY_MS
    const rangeStartDate = new Date(rangeStartMs)
    const alignedStartMs = rangeStartMs - rangeStartDate.getUTCDay() * DAY_MS
    const dayCount = trackedDays + rangeStartDate.getUTCDay()
    const columnCount = Math.max(1, Math.ceil(dayCount / 7))
    const monthLabels: Array<{ key: string; label: string; x: number }> = []
    let previousMonth: number | null = null
    let lastMonthLabelX = Number.NEGATIVE_INFINITY

    const rawCells = Array.from({ length: dayCount }, (_, index) => {
      const timestamp = alignedStartMs + index * DAY_MS
      const point = pointsByDay.get(timestamp) ?? {
        day_start_ms: timestamp,
        total_requests: 0,
        total_sessions: 0,
        total_cost_usd: 0,
        total_tokens: 0,
        total_lines_changed: 0,
      }
      const date = new Date(timestamp)
      const month = date.getUTCMonth()
      const column = Math.floor(index / 7)
      const row = date.getUTCDay()
      const x = leftAxisWidth + column * (cellSize + columnGap)
      const score =
        point.total_requests +
        point.total_sessions * 12 +
        point.total_lines_changed / 200

      if (month !== previousMonth) {
        const shouldSkipPartialLeadingMonth =
          monthLabels.length === 0 && date.getUTCDate() > 7
        if (!shouldSkipPartialLeadingMonth && x - lastMonthLabelX >= 28) {
          monthLabels.push({
            key: `${date.getFullYear()}-${month}`,
            label: monthFormatter.format(date),
            x,
          })
          lastMonthLabelX = x
        }
        previousMonth = month
      }

      return {
        key: point.day_start_ms,
        point,
        score,
        x,
        y: topAxisHeight + row * (cellSize + rowGap),
      }
    })

    const positiveScores = rawCells
      .map((cell) => cell.score)
      .filter((score) => score > 0)
      .sort((left, right) => left - right)

    const renderedCells = rawCells.map((cell) => {
      let level: 0 | 1 | 2 | 3 | 4 = 0

      if (cell.score > 0) {
        if (positiveScores.length === 1) {
          level = 4
        } else {
          const percentile =
            positiveScores.filter((score) => score <= cell.score).length /
            positiveScores.length
          level =
            percentile <= 0.25
              ? 1
              : percentile <= 0.5
                ? 2
                : percentile <= 0.75
                  ? 3
                  : 4
        }
      }

      return {
        ...cell,
        level,
      }
    })

    return {
      width:
        leftAxisWidth +
        columnCount * cellSize +
        Math.max(0, columnCount - 1) * columnGap,
      height: topAxisHeight + 7 * cellSize + 6 * rowGap,
      leftAxisWidth,
      topAxisHeight,
      cellSize,
      columnGap,
      rowGap,
      cellRadius,
      monthLabelY,
      weekdayLabelOffset,
      monthLabels,
      cells: renderedCells,
    }
  }, [cells, compact])

  return (
    <section className={cn("space-y-3", compact && "space-y-2")}>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
            Yearly activity context
          </p>
          <p
            className={cn(
              "font-semibold tracking-[-0.04em] text-foreground",
              compact ? "mt-1 text-[14px]" : "mt-1.5 text-[18px]"
            )}
          >
            {formatCount(activeDays.length)} active days in 12 months
          </p>
          {!compact ? (
            <p className="mt-1 text-[11px] text-muted-foreground">
              Secondary context for seasonality; keep recent throughput and
              workspace drilldown as primary signals.
            </p>
          ) : null}
        </div>

        <div
          className={cn(
            "flex items-center text-muted-foreground",
            compact ? "gap-1.5 text-[10px]" : "gap-2 text-[11px]"
          )}
        >
          <span>Less</span>
          {[0, 1, 2, 3, 4].map((level) => (
            <span
              key={level}
              className={cn(
                "border border-border/20",
                compact ? "size-2.5 rounded-[3px]" : "size-3 rounded-[4px]"
              )}
              style={{
                backgroundColor: `var(--trace-heatmap-level-${level})`,
              }}
            />
          ))}
          <span>More</span>
        </div>
      </div>

      <div className={cn(compact ? "mt-2 space-y-1.5" : "mt-3.5 space-y-2.5")}>
        <div
          className={cn(
            "trace-overview-chart border border-border/18 bg-muted/[0.04]",
            compact ? "rounded-[12px] p-2" : "rounded-[20px] p-3"
          )}
          onMouseLeave={() => setActiveDayStartMs(null)}
        >
          <svg
            viewBox={`0 0 ${heatmap.width} ${heatmap.height}`}
            className={cn("block h-auto", compact ? "w-full" : "max-w-full")}
            preserveAspectRatio="xMinYMin meet"
            aria-hidden="true"
            style={compact ? undefined : { width: `${heatmap.width}px` }}
          >
            {heatmap.monthLabels.map((month) => (
              <text
                key={month.key}
                x={month.x}
                y={heatmap.monthLabelY}
                fill="currentColor"
                className={cn(
                  "text-muted-foreground",
                  compact ? "text-[9px]" : "text-[10px]"
                )}
              >
                {month.label}
              </text>
            ))}

            {HEATMAP_WEEKDAY_LABELS.map((weekday) => (
              <text
                key={weekday.label}
                x="0"
                y={
                  heatmap.topAxisHeight +
                  weekday.row * (heatmap.cellSize + heatmap.rowGap) +
                  heatmap.weekdayLabelOffset
                }
                fill="currentColor"
                className={cn(
                  "text-muted-foreground",
                  compact ? "text-[9px]" : "text-[10px]"
                )}
              >
                {weekday.label}
              </text>
            ))}

            {heatmap.cells.map((cell) => (
              <g key={cell.key}>
                <title>
                  {`${monthDayFormatter.format(
                    new Date(cell.point.day_start_ms)
                  )} · ${formatCount(cell.point.total_requests)} requests · ${formatCount(
                    cell.point.total_sessions
                  )} sessions · ${formatCompactCount(
                    cell.point.total_lines_changed
                  )} lines changed`}
                </title>
                <rect
                  x={cell.x}
                  y={cell.y}
                  width={heatmap.cellSize}
                  height={heatmap.cellSize}
                  rx={heatmap.cellRadius}
                  className="trace-overview-heatmap-cell"
                  fill={`var(--trace-heatmap-level-${cell.level})`}
                  stroke={
                    highlightedDay?.day_start_ms === cell.point.day_start_ms
                      ? "var(--trace-accent-strong)"
                      : "color-mix(in oklab, var(--border) 58%, transparent)"
                  }
                  strokeWidth={
                    highlightedDay?.day_start_ms === cell.point.day_start_ms
                      ? compact
                        ? 1
                        : 1.2
                      : 0.9
                  }
                  cursor="pointer"
                  onMouseEnter={() =>
                    setActiveDayStartMs(cell.point.day_start_ms)
                  }
                  onClick={() => setActiveDayStartMs(cell.point.day_start_ms)}
                />
              </g>
            ))}
          </svg>
        </div>

        <div
          className={cn(
            "flex flex-wrap items-center gap-x-3 gap-y-1.5",
            compact ? "text-[9px]" : "text-[11px]"
          )}
        >
          <span className="font-medium text-foreground">
            {activeDayStartMs != null ? "Hovered" : "Latest"}{" "}
            {highlightedDay
              ? monthDayFormatter.format(new Date(highlightedDay.day_start_ms))
              : "-"}
          </span>
          <span className="text-muted-foreground">
            {highlightedDay
              ? `${formatCount(highlightedDay.total_requests)} requests · ${formatCount(highlightedDay.total_sessions)} sessions`
              : "No activity captured."}
          </span>
          {!compact ? (
            <>
              <span className="rounded-full border border-border/18 bg-background/60 px-2 py-0.5 text-muted-foreground">
                Tokens {formatCompactCount(highlightedDay?.total_tokens)}
              </span>
              <span className="rounded-full border border-border/18 bg-background/60 px-2 py-0.5 text-muted-foreground">
                Cost {formatUsd(highlightedDay?.total_cost_usd)}
              </span>
              <span className="rounded-full border border-border/18 bg-background/60 px-2 py-0.5 text-muted-foreground">
                Lines {formatCompactCount(highlightedDay?.total_lines_changed)}
              </span>
            </>
          ) : (
            <span className="rounded-full border border-border/18 bg-background/60 px-2 py-0.5 text-muted-foreground">
              Tokens {formatCompactCount(highlightedDay?.total_tokens)}
            </span>
          )}
          <span className="rounded-full border border-border/18 bg-background/60 px-2 py-0.5 text-muted-foreground">
            Rolling {formatCount(recentActiveDays)} / 30d
          </span>
        </div>
      </div>
    </section>
  )
}

export function TraceOverviewPanel() {
  const dashboard = useTraceOverviewStore((state) => state.dashboard)
  const loading = useTraceOverviewStore((state) => state.loading)
  const error = useTraceOverviewStore((state) => state.error)
  const initialize = useTraceOverviewStore((state) => state.initialize)
  const openWorkspace = useTraceStore((state) => state.openWorkspace)

  useEffect(() => {
    void initialize().catch(() => {})
  }, [initialize])

  useEffect(() => {
    document.body.classList.add("trace-overview-active")
    return () => {
      document.body.classList.remove("trace-overview-active")
    }
  }, [])

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-3.5 pt-1 pb-2.5">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[1480px] flex-col gap-2">
          {error ? (
            <div className="rounded-2xl border border-destructive/20 bg-destructive/[0.05] px-4 py-3 text-[12px] text-destructive">
              {error}
            </div>
          ) : null}

          {dashboard ? (
            <>
              <OverviewHeroCard
                dashboard={dashboard}
                onOpenConversation={() => void openWorkspace("conversation")}
                onOpenCompression={() => void openWorkspace("compression")}
              />

              <section className="trace-overview-card trace-overview-section trace-overview-primary rounded-[20px] border border-border/25 bg-card/95 p-3 md:p-4">
                <TrendChartBlock dashboard={dashboard} />
              </section>
            </>
          ) : null}

          {loading && !dashboard ? (
            <div className="trace-overview-card rounded-2xl border border-border/25 bg-card px-4 py-6 text-[12px] text-muted-foreground">
              Loading dashboard analytics...
            </div>
          ) : null}

          {!loading && !dashboard && !error ? (
            <section className="trace-overview-card rounded-[28px] border border-border/25 bg-card/95 px-5 py-6">
              <p className="text-[12px] text-muted-foreground">
                No dashboard data available yet.
              </p>
            </section>
          ) : null}
        </div>
      </div>
    </div>
  )
}
