import { useEffect } from "react"
import { ArrowRight } from "lucide-react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { TraceSummary } from "@/lib/types"
import { useTraceOverviewStore } from "@/stores/trace-overview-store"
import { useTraceStore } from "@/stores/trace-store"

const compactNumberFormatter = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
})

function formatCount(value: number | null | undefined) {
  return value != null ? value.toLocaleString("en-US") : "-"
}

function formatCompactCount(value: number | null | undefined) {
  return value != null ? compactNumberFormatter.format(value) : "-"
}

function formatDuration(value: number | null | undefined) {
  if (value == null) return "-"
  if (value < 1000) return `${value} ms`
  if (value < 60_000) return `${(value / 1000).toFixed(1)} s`

  const minutes = Math.floor(value / 60_000)
  const seconds = Math.floor((value % 60_000) / 1000)
  return `${minutes}m ${seconds}s`
}

function formatPercent(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-"
  const percent = value * 100
  if (percent >= 100) return "100%"
  if (percent >= 10) return `${Math.round(percent)}%`
  return `${percent.toFixed(1)}%`
}

function formatDecimal(value: number | null | undefined, digits = 1) {
  if (value == null || !Number.isFinite(value)) return "-"
  return value.toFixed(digits).replace(/\.0$/, "")
}

function formatActivityAt(value: number | null | undefined) {
  if (value == null) return "-"
  return new Date(value).toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  })
}

function ratio(numerator: number, denominator: number) {
  if (denominator <= 0) return null
  return numerator / denominator
}

function completedRequests(summary: TraceSummary | null) {
  if (!summary) return 0
  return Math.max(
    0,
    summary.total_requests - summary.failed_requests - summary.partial_requests
  )
}

function healthRate(summary: TraceSummary | null) {
  if (!summary) return null
  return ratio(completedRequests(summary), summary.total_requests)
}

function toolReach(summary: TraceSummary | null) {
  if (!summary) return null
  return ratio(summary.requests_with_tools, summary.total_requests)
}

function cacheReuse(summary: TraceSummary | null) {
  if (!summary) return null
  return ratio(summary.total_cached_tokens, summary.total_input_tokens)
}

function toolCallsPerRequest(summary: TraceSummary | null) {
  if (!summary) return null
  return ratio(summary.total_tool_spans, summary.total_requests)
}

function averageTokens(summary: TraceSummary | null) {
  if (!summary) return null
  return ratio(summary.total_tokens, summary.total_requests)
}

function spanLoad(summary: TraceSummary | null) {
  if (!summary) return 0
  return summary.total_llm_spans + summary.total_tool_spans
}

function OverviewMetric({
  label,
  value,
  detail,
}: {
  label: string
  value: string
  detail: string
}) {
  return (
    <div className="space-y-1 px-4 py-3">
      <p className="text-[10px] tracking-[0.12em] text-muted-foreground uppercase">
        {label}
      </p>
      <p className="text-[22px] font-semibold tracking-tight text-foreground tabular-nums">
        {value}
      </p>
      <p className="text-[11px] leading-5 text-muted-foreground">{detail}</p>
    </div>
  )
}

function OverviewLaneStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <p className="text-[10px] tracking-[0.12em] text-muted-foreground uppercase">
        {label}
      </p>
      <p className="mt-1.5 text-[18px] font-semibold tracking-tight text-foreground tabular-nums">
        {value}
      </p>
    </div>
  )
}

function OverviewLane({
  title,
  description,
  summary,
  accent,
  onOpen,
}: {
  title: string
  description: string
  summary: TraceSummary | null
  accent: "sky" | "amber"
  onOpen: () => void
}) {
  const accentClasses =
    accent === "sky"
      ? "border-sky-500/20 bg-linear-to-br from-sky-500/[0.08] via-background/80 to-background"
      : "border-amber-500/20 bg-linear-to-br from-amber-500/[0.08] via-background/80 to-background"

  return (
    <section className={cn("rounded-[28px] border p-4", accentClasses)}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[10px] tracking-[0.12em] text-muted-foreground uppercase">
            {title}
          </p>
          <h3 className="mt-2 text-[24px] font-semibold tracking-tight text-foreground">
            {formatCount(summary?.total_requests)} requests
          </h3>
          <p className="mt-2 max-w-xl text-[12px] leading-6 text-muted-foreground">
            {description}
          </p>
        </div>

        <Button variant="outline" size="sm" onClick={onOpen}>
          Open explorer
          <ArrowRight className="size-3.5" />
        </Button>
      </div>

      <div className="mt-5 grid gap-x-4 gap-y-3 border-t border-border/15 pt-4 sm:grid-cols-2 xl:grid-cols-5">
        <OverviewLaneStat
          label="requests"
          value={formatCount(summary?.total_requests)}
        />
        <OverviewLaneStat
          label="health"
          value={formatPercent(healthRate(summary))}
        />
        <OverviewLaneStat
          label="latency"
          value={formatDuration(summary?.p95_duration_ms)}
        />
        <OverviewLaneStat
          label="tool reach"
          value={formatPercent(toolReach(summary))}
        />
        <OverviewLaneStat
          label="token load"
          value={formatCompactCount(summary?.total_tokens)}
        />
        <OverviewLaneStat
          label="cache reuse"
          value={formatPercent(cacheReuse(summary))}
        />
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
        <span>
          {formatDecimal(toolCallsPerRequest(summary))} calls / request
        </span>
        <span className="text-border">/</span>
        <span>{formatDecimal(averageTokens(summary))} tok / request</span>
        <span className="text-border">/</span>
        <span>{formatCount(summary?.unique_models)} models</span>
        <span className="text-border">/</span>
        <span>{formatCount(summary?.total_llm_spans)} llm spans</span>
        <span className="text-border">/</span>
        <span>{formatCount(summary?.total_tool_spans)} tool spans</span>
        <span className="text-border">/</span>
        <span>
          last trace {formatActivityAt(summary?.latest_request_started_at_ms)}
        </span>
      </div>
    </section>
  )
}

export function TraceOverviewPanel() {
  const overallSummary = useTraceOverviewStore((state) => state.overallSummary)
  const conversationSummary = useTraceOverviewStore(
    (state) => state.conversationSummary
  )
  const compressionSummary = useTraceOverviewStore(
    (state) => state.compressionSummary
  )
  const loading = useTraceOverviewStore((state) => state.loading)
  const error = useTraceOverviewStore((state) => state.error)
  const initialize = useTraceOverviewStore((state) => state.initialize)
  const openWorkspace = useTraceStore((state) => state.openWorkspace)

  useEffect(() => {
    void initialize().catch(() => {})
  }, [initialize])

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-5 py-4">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[1440px] flex-col gap-4">
          <section className="overflow-hidden rounded-[30px] border border-border/35 bg-linear-to-br from-background via-background to-muted/20">
            <div className="grid md:grid-cols-2 xl:grid-cols-4 xl:divide-x xl:divide-border/15">
              <OverviewMetric
                label="requests"
                value={formatCount(overallSummary?.total_requests)}
                detail={`${formatCount(completedRequests(overallSummary))} completed across all recorded loops`}
              />
              <OverviewMetric
                label="health"
                value={formatPercent(healthRate(overallSummary))}
                detail={`${formatCount(overallSummary?.failed_requests)} failed · ${formatCount(
                  overallSummary?.partial_requests
                )} partial`}
              />
              <OverviewMetric
                label="latency"
                value={formatDuration(overallSummary?.p95_duration_ms)}
                detail={`avg ${formatDuration(overallSummary?.avg_duration_ms)}`}
              />
              <OverviewMetric
                label="context reuse"
                value={formatPercent(cacheReuse(overallSummary))}
                detail={`${formatCompactCount(
                  overallSummary?.total_cached_tokens
                )} cached / ${formatCompactCount(
                  overallSummary?.total_input_tokens
                )} input tokens`}
              />
            </div>

            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-t border-border/15 px-5 py-3 text-[11px] text-muted-foreground">
              <span>{formatCount(spanLoad(overallSummary))} total spans</span>
              <span className="text-border">/</span>
              <span>{formatPercent(toolReach(overallSummary))} tool reach</span>
              <span className="text-border">/</span>
              <span>
                {formatDecimal(toolCallsPerRequest(overallSummary))} calls /
                request
              </span>
              <span className="text-border">/</span>
              <span>
                {formatCompactCount(overallSummary?.total_tokens)} total tok
              </span>
            </div>
          </section>

          {error ? (
            <div className="rounded-2xl border border-destructive/20 bg-destructive/[0.05] px-4 py-3 text-[12px] text-destructive">
              {error}
            </div>
          ) : null}

          <section className="grid gap-4 xl:grid-cols-2">
            <OverviewLane
              title="Conversation"
              description="Primary agent loops and end-user prompts. Use this explorer when you need waterfall depth, span ordering, and payload inspection."
              summary={conversationSummary}
              accent="sky"
              onOpen={() => void openWorkspace("conversation")}
            />
            <OverviewLane
              title="Compression"
              description="Context compaction runs and summary generation calls. Use this explorer when you want to audit memory pressure or summary quality regressions."
              summary={compressionSummary}
              accent="amber"
              onOpen={() => void openWorkspace("compression")}
            />
          </section>

          {loading && !overallSummary ? (
            <div className="rounded-2xl border border-border/30 bg-background/65 px-4 py-6 text-[12px] text-muted-foreground">
              Loading cumulative trace overview...
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}
