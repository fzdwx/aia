import { useEffect, useState } from "react"
import {
  ArrowLeft,
  ChevronRight,
  Database,
  RefreshCw,
  Workflow,
} from "lucide-react"

import { TraceDetailModal } from "@/components/trace-detail-modal"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import type { TraceListItem } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { useTraceStore } from "@/stores/trace-store"

type LoopGroup = {
  key: string
  turnId: string
  runId: string
  userMessage: string | null
  model: string
  protocol: string
  endpointPath: string
  latestStartedAtMs: number
  totalDurationMs: number
  totalTokens: number
  stepCount: number
  finalStatus: "completed" | "failed" | "partial"
  traces: TraceListItem[]
  pathSummary: string
  latestError: string | null
}

function formatDateTime(value: number) {
  return new Date(value).toLocaleString("en-US", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  })
}

function MetricCard({
  label,
  value,
  icon,
}: {
  label: string
  value: string
  icon: React.ReactNode
}) {
  return (
    <Card size="sm" className="gap-2">
      <CardHeader className="flex flex-row items-center justify-between pb-0">
        <CardDescription className="text-[11px] tracking-wide uppercase">
          {label}
        </CardDescription>
        <div className="text-muted-foreground">{icon}</div>
      </CardHeader>
      <CardContent>
        <div className="text-lg font-semibold tabular-nums">{value}</div>
      </CardContent>
    </Card>
  )
}

function truncate(text: string, max: number) {
  if (text.length <= max) return text
  return `${text.slice(0, max - 1)}...`
}

function summarizeStopReason(trace: TraceListItem) {
  if (trace.status === "failed") return "failed"
  return trace.stop_reason ?? "completed"
}

function summarizeLoopStatus(traces: TraceListItem[]) {
  const sorted = [...traces].sort(
    (left, right) => left.step_index - right.step_index
  )
  const finalTrace = sorted[sorted.length - 1]
  const hasFailure = sorted.some((trace) => trace.status === "failed")

  if (finalTrace?.status === "failed") return "failed" as const
  if (hasFailure) return "partial" as const
  return "completed" as const
}

function buildLoopGroups(traces: TraceListItem[]): LoopGroup[] {
  const groups = new Map<string, TraceListItem[]>()

  for (const trace of traces) {
    const key = `${trace.turn_id}:${trace.run_id}`
    const items = groups.get(key)
    if (items) {
      items.push(trace)
    } else {
      groups.set(key, [trace])
    }
  }

  return Array.from(groups.entries())
    .map(([key, items]) => {
      const tracesByStep = [...items].sort(
        (left, right) => left.step_index - right.step_index
      )
      const latestTrace = [...items].sort(
        (left, right) => right.started_at_ms - left.started_at_ms
      )[0]

      return {
        key,
        turnId: latestTrace.turn_id,
        runId: latestTrace.run_id,
        userMessage:
          tracesByStep.find((trace) => trace.user_message)?.user_message ??
          null,
        model: latestTrace.model,
        protocol: latestTrace.protocol,
        endpointPath: latestTrace.endpoint_path,
        latestStartedAtMs: latestTrace.started_at_ms,
        totalDurationMs: tracesByStep.reduce(
          (sum, trace) => sum + (trace.duration_ms ?? 0),
          0
        ),
        totalTokens: tracesByStep.reduce(
          (sum, trace) => sum + (trace.total_tokens ?? 0),
          0
        ),
        stepCount: tracesByStep.length,
        finalStatus: summarizeLoopStatus(tracesByStep),
        traces: tracesByStep,
        pathSummary: tracesByStep.map(summarizeStopReason).join(" -> "),
        latestError:
          [...tracesByStep].reverse().find((trace) => trace.error)?.error ??
          null,
      }
    })
    .sort((left, right) => right.latestStartedAtMs - left.latestStartedAtMs)
}

function loopStatusVariant(status: LoopGroup["finalStatus"]) {
  switch (status) {
    case "failed":
      return "destructive" as const
    case "partial":
      return "outline" as const
    default:
      return "secondary" as const
  }
}

function traceStatusVariant(status: TraceListItem["status"]) {
  return status === "failed" ? ("destructive" as const) : ("secondary" as const)
}

export function TracePanel() {
  const setView = useChatStore((s) => s.setView)
  const traces = useTraceStore((s) => s.traces)
  const selectedTraceId = useTraceStore((s) => s.selectedTraceId)
  const selectedTrace = useTraceStore((s) => s.selectedTrace)
  const traceSummary = useTraceStore((s) => s.traceSummary)
  const traceLoading = useTraceStore((s) => s.traceLoading)
  const traceError = useTraceStore((s) => s.traceError)
  const refreshTraces = useTraceStore((s) => s.refreshTraces)
  const selectTrace = useTraceStore((s) => s.selectTrace)
  const clearSelection = useTraceStore((s) => s.clearSelection)
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({})

  useEffect(() => {
    refreshTraces().catch(() => {})
  }, [refreshTraces])

  const loopGroups = buildLoopGroups(traces)

  useEffect(() => {
    setOpenGroups((previous) => {
      const next = { ...previous }
      for (const group of loopGroups) {
        if (!(group.key in next)) {
          next[group.key] = false
        }
      }
      return next
    })
  }, [loopGroups])

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="mx-auto max-w-[1180px] px-6 py-8">
        <div className="mb-8 flex items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setView("chat")}
              className="flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
            >
              <ArrowLeft className="size-4" />
            </button>
            <div>
              <h1 className="text-lg font-semibold">Trace</h1>
              <p className="text-[13px] text-muted-foreground">
                Inspect agent loops first, then drill into the individual LLM
                calls inside each loop.
              </p>
            </div>
          </div>
          <Button variant="outline" size="sm" onClick={() => refreshTraces()}>
            <RefreshCw className="size-3.5" />
            Refresh
          </Button>
        </div>

        <div className="mb-6 grid gap-3 md:grid-cols-4">
          <MetricCard
            label="requests"
            value={String(traceSummary?.total_requests ?? 0)}
            icon={<Workflow className="size-4" />}
          />
          <MetricCard
            label="failed"
            value={String(traceSummary?.failed_requests ?? 0)}
            icon={<Badge variant="destructive">ERR</Badge>}
          />
          <MetricCard
            label="avg latency"
            value={
              traceSummary?.avg_duration_ms != null
                ? `${traceSummary.avg_duration_ms.toFixed(1)} ms`
                : "-"
            }
            icon={<Database className="size-4" />}
          />
          <MetricCard
            label="tokens"
            value={String(traceSummary?.total_tokens ?? 0)}
            icon={<span className="text-xs font-medium">TOK</span>}
          />
        </div>

        {traceError && (
          <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[13px] text-destructive">
            {traceError}
          </div>
        )}

        <div className="grid gap-6">
          <Card className="min-h-[520px] gap-0">
            <CardHeader className="pb-3">
              <CardTitle>Recent loops</CardTitle>
              <CardDescription>
                Traces grouped by agent loop. Expand a loop to inspect
                step-by-step LLM calls, then click a step for the full payload
                detail modal.
              </CardDescription>
            </CardHeader>
            <Separator className="opacity-40" />
            <CardContent className="px-0">
              <div className="space-y-2 px-2">
                {loopGroups.map((group) => (
                  <Collapsible
                    key={group.key}
                    open={openGroups[group.key] ?? false}
                    onOpenChange={(open) =>
                      setOpenGroups((previous) => ({
                        ...previous,
                        [group.key]: open,
                      }))
                    }
                    className="rounded-xl border border-border/50 bg-background/80"
                  >
                    <CollapsibleTrigger className="w-full text-left">
                      <div className="rounded-xl px-4 py-4 transition-colors hover:bg-accent/30">
                        <div className="flex items-start justify-between gap-4">
                          <div className="min-w-0 space-y-1.5 flex-1">
                            <div className="flex flex-wrap items-center gap-2">
                              <ChevronRight
                                className={cn(
                                  "size-4 text-muted-foreground transition-transform",
                                  openGroups[group.key] && "rotate-90"
                                )}
                              />
                              <span className="text-[13px] font-semibold text-foreground">
                                Loop {group.turnId}
                              </span>
                              {group.turnId !== group.runId ? (
                                <Badge
                                  variant="outline"
                                  className="text-[10px]"
                                >
                                  run {group.runId}
                                </Badge>
                              ) : null}
                              <Badge
                                variant={loopStatusVariant(group.finalStatus)}
                                className="text-[10px]"
                              >
                                {group.finalStatus}
                              </Badge>
                            </div>
                            <div className="text-[12px] text-muted-foreground">
                              {group.model} · {group.endpointPath} ·{" "}
                              {group.protocol}
                            </div>
                            {group.userMessage ? (
                              <p className="max-w-[780px] text-[13px] leading-5 text-foreground/85">
                                {truncate(group.userMessage, 180)}
                              </p>
                            ) : (
                              <p className="text-[12px] text-muted-foreground">
                                User message unavailable.
                              </p>
                            )}
                          </div>
                          <div className="shrink-0 text-right text-[11px] text-muted-foreground">
                            <div className="font-medium text-foreground/90">{formatDateTime(group.latestStartedAtMs)}</div>
                            <div className="mt-1 space-x-2">
                              <span>{group.stepCount} calls</span>
                              <span>·</span>
                              <span>{group.totalDurationMs}ms</span>
                            </div>
                            <div className="mt-0.5">{group.totalTokens} tokens</div>
                          </div>
                        </div>

                        {group.pathSummary ? (
                          <div className="mt-2 text-[11px] text-muted-foreground/80">
                            <span className="truncate rounded-full bg-muted/50 px-2 py-0.5">
                              {group.pathSummary}
                            </span>
                          </div>
                        ) : null}

                        {group.latestError ? (
                          <p className="mt-2 line-clamp-2 text-[11px] text-destructive">
                            {group.latestError}
                          </p>
                        ) : null}
                      </div>
                    </CollapsibleTrigger>

                    <CollapsibleContent className="border-t border-border/40 px-3 py-3">
                      <div className="space-y-2">
                        {group.traces.map((trace) => (
                          <button
                            key={trace.id}
                            onClick={() => selectTrace(trace.id)}
                            className={cn(
                              "w-full rounded-lg border border-border/40 bg-background px-3 py-3 text-left transition-colors hover:bg-accent/30",
                              selectedTraceId === trace.id && "bg-accent/40"
                            )}
                          >
                            <div className="flex items-start justify-between gap-3">
                              <div className="min-w-0 space-y-1">
                                <div className="flex flex-wrap items-center gap-2">
                                  <span className="text-[12px] font-medium text-foreground">
                                    step {trace.step_index}
                                  </span>
                                  <Badge
                                    variant={traceStatusVariant(trace.status)}
                                    className="text-[10px]"
                                  >
                                    {trace.status}
                                  </Badge>
                                  <Badge
                                    variant="outline"
                                    className="text-[10px]"
                                  >
                                    {trace.request_kind}
                                  </Badge>
                                  {trace.stop_reason ? (
                                    <Badge
                                      variant="outline"
                                      className="text-[10px]"
                                    >
                                      {trace.stop_reason}
                                    </Badge>
                                  ) : null}
                                </div>
                                <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                                  <span>
                                    {formatDateTime(trace.started_at_ms)}
                                  </span>
                                  <span>{trace.duration_ms ?? "-"} ms</span>
                                  <span>{trace.total_tokens ?? 0} tokens</span>
                                  {trace.status_code != null ? (
                                    <span>HTTP {trace.status_code}</span>
                                  ) : null}
                                </div>
                              </div>
                            </div>
                            {trace.error ? (
                              <p className="mt-2 line-clamp-2 text-[11px] text-destructive">
                                {trace.error}
                              </p>
                            ) : null}
                          </button>
                        ))}
                      </div>
                    </CollapsibleContent>
                  </Collapsible>
                ))}
                {loopGroups.length === 0 && !traceLoading && (
                  <p className="px-3 py-6 text-sm text-muted-foreground">
                    No traces recorded yet.
                  </p>
                )}
              </div>
            </CardContent>
          </Card>
        </div>

        <TraceDetailModal
          open={selectedTraceId != null}
          trace={selectedTrace}
          loading={traceLoading}
          onOpenChange={(open) => {
            if (!open) {
              clearSelection()
            }
          }}
        />
      </div>
    </ScrollArea>
  )
}
