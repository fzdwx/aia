import { useEffect, useMemo } from "react"

import { cn } from "@/lib/utils"
import {
  buildTraceLoopGroups,
  partitionTraceLoopGroups,
  type TraceLoopGroup,
} from "@/lib/trace-presentation"
import { useChatStore } from "@/stores/chat-store"
import { useTraceStore } from "@/stores/trace-store"

function formatCompactDateTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  })
}

function formatDuration(value: number | null | undefined) {
  if (value == null) return "-"
  if (value < 1000) return `${value} ms`
  if (value < 60_000) return `${(value / 1000).toFixed(1)} s`

  const minutes = Math.floor(value / 60_000)
  const seconds = Math.floor((value % 60_000) / 1000)
  return `${minutes}m ${seconds}s`
}

function truncate(text: string, max: number) {
  if (text.length <= max) return text
  return `${text.slice(0, max - 1)}...`
}

function loopHeadline(group: TraceLoopGroup) {
  if (group.requestKind === "compression") {
    return "Context compression"
  }

  return truncate(group.userMessage ?? "User message unavailable.", 64)
}

function loopStatusTone(group: TraceLoopGroup) {
  switch (group.finalStatus) {
    case "failed":
      return "bg-destructive"
    case "partial":
      return "bg-amber-500"
    default:
      return "bg-emerald-500"
  }
}

export function TraceSidebar() {
  const turns = useChatStore((state) => state.turns)
  const traces = useTraceStore((state) => state.traces)
  const traceView = useTraceStore((state) => state.traceView)
  const traceLoading = useTraceStore((state) => state.traceLoading)
  const traceError = useTraceStore((state) => state.traceError)
  const tracePage = useTraceStore((state) => state.tracePage)
  const tracePageSize = useTraceStore((state) => state.tracePageSize)
  const totalTraceItems = useTraceStore((state) => state.totalTraceItems)
  const activeLoopKey = useTraceStore((state) => state.activeLoopKey)
  const refreshTraces = useTraceStore((state) => state.refreshTraces)
  const switchTraceView = useTraceStore((state) => state.switchTraceView)
  const selectLoop = useTraceStore((state) => state.selectLoop)

  useEffect(() => {
    void refreshTraces().catch(() => {})
  }, [refreshTraces])

  const loopGroups = useMemo(
    () => buildTraceLoopGroups(traces, turns),
    [traces, turns]
  )
  const partitionedGroups = useMemo(
    () => partitionTraceLoopGroups(loopGroups),
    [loopGroups]
  )
  const visibleLoopGroups =
    traceView === "compression"
      ? partitionedGroups.compression
      : partitionedGroups.conversation

  const resolvedActiveLoopKey =
    activeLoopKey &&
    visibleLoopGroups.some((group) => group.key === activeLoopKey)
      ? activeLoopKey
      : (visibleLoopGroups[0]?.key ?? null)

  const pageCount = Math.max(1, Math.ceil(totalTraceItems / tracePageSize))

  return (
    <>
      <div className="px-2 pt-2">
        <div className="rounded-lg border border-border/30 bg-muted/20 px-3 py-2.5">
          <p className="text-[13px] font-medium text-foreground/80">Traces</p>
          <div className="mt-2 flex items-center gap-1">
            <button
              type="button"
              onClick={() => switchTraceView("conversation").catch(() => {})}
              className={cn(
                "rounded-md px-2 py-1 text-[11px] transition-colors",
                traceView === "conversation"
                  ? "bg-accent/50 text-foreground/85"
                  : "text-muted-foreground hover:bg-accent/30 hover:text-foreground/80"
              )}
            >
              Conversation
            </button>
            <button
              type="button"
              onClick={() => switchTraceView("compression").catch(() => {})}
              className={cn(
                "rounded-md px-2 py-1 text-[11px] transition-colors",
                traceView === "compression"
                  ? "bg-accent/50 text-foreground/85"
                  : "text-muted-foreground hover:bg-accent/30 hover:text-foreground/80"
              )}
            >
              Compression
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 pt-2 pb-2">
        {traceError && visibleLoopGroups.length === 0 ? (
          <p className="px-2.5 py-2 text-[12px] leading-5 text-destructive">
            {traceError}
          </p>
        ) : null}

        {traceLoading && visibleLoopGroups.length === 0 ? (
          <p className="px-2.5 py-2 text-[12px] text-muted-foreground">
            Loading traces...
          </p>
        ) : null}

        {!traceLoading && visibleLoopGroups.length === 0 ? (
          <p className="px-2.5 py-2 text-[12px] text-muted-foreground">
            {traceView === "compression"
              ? "No compression logs available."
              : "No traces available."}
          </p>
        ) : null}

        {visibleLoopGroups.map((group) => {
          const isActive = group.key === resolvedActiveLoopKey
          const defaultNodeId =
            group.finalSpanId ?? group.timeline[0]?.id ?? `${group.key}:root`

          return (
            <button
              key={group.key}
              type="button"
              onClick={() => selectLoop(group.key, defaultNodeId)}
              className={cn(
                "mb-1.5 flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors duration-150",
                isActive
                  ? "bg-accent/50 text-foreground/85"
                  : "text-muted-foreground hover:bg-accent/30 hover:text-foreground/80"
              )}
            >
              <span
                className={cn(
                  "mt-1 size-1.5 shrink-0 rounded-full",
                  loopStatusTone(group)
                )}
              />
              <span className="min-w-0 flex-1">
                <span className="line-clamp-2 text-[12px] leading-4">
                  {loopHeadline(group)}
                </span>
                <span className="mt-1 block text-[11px] text-muted-foreground/80">
                  {formatCompactDateTime(group.latestStartedAtMs)} ·{" "}
                  {formatDuration(group.totalDurationMs)}
                </span>
                <span className="mt-0.5 block text-[11px] text-muted-foreground/65">
                  {group.stepCount} llm · {group.toolCount} tool
                </span>
              </span>
            </button>
          )
        })}
      </div>

      {pageCount > 1 ? (
        <div className="px-2 pb-2">
          <div className="flex items-center justify-between rounded-lg border border-border/30 bg-muted/20 px-2.5 py-2">
            <button
              type="button"
              onClick={() => refreshTraces({ page: tracePage - 1 })}
              disabled={tracePage <= 1 || traceLoading}
              className="text-[11px] text-muted-foreground transition-colors hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
            >
              Prev
            </button>
            <span className="text-[11px] text-muted-foreground">
              {tracePage}/{pageCount}
            </span>
            <button
              type="button"
              onClick={() => refreshTraces({ page: tracePage + 1 })}
              disabled={tracePage >= pageCount || traceLoading}
              className="text-[11px] text-muted-foreground transition-colors hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
            >
              Next
            </button>
          </div>
        </div>
      ) : null}
    </>
  )
}
