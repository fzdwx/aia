import { Bot, Layers3, Waypoints } from "lucide-react"
import { useMemo } from "react"

import { cn } from "@/lib/utils"
import {
  buildTraceLoopGroups,
  formatTraceDuration,
  formatTraceLoopHeadline,
  resolveActiveTraceLoopKey,
  selectVisibleTraceLoopGroups,
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

function loopStatusTone(group: {
  finalStatus: "completed" | "failed" | "partial"
}) {
  switch (group.finalStatus) {
    case "failed":
      return "bg-destructive"
    case "partial":
      return "bg-amber-500"
    default:
      return "bg-foreground/40"
  }
}

export function TraceSidebar() {
  const turns = useChatStore((state) => state.turns)
  const traces = useTraceStore((state) => state.traces)
  const traceSurface = useTraceStore((state) => state.traceSurface)
  const traceView = useTraceStore((state) => state.traceView)
  const traceLoading = useTraceStore((state) => state.traceLoading)
  const traceError = useTraceStore((state) => state.traceError)
  const tracePage = useTraceStore((state) => state.tracePage)
  const tracePageSize = useTraceStore((state) => state.tracePageSize)
  const totalTraceItems = useTraceStore((state) => state.totalTraceItems)
  const activeLoopKey = useTraceStore((state) => state.activeLoopKey)
  const refreshTraces = useTraceStore((state) => state.refreshTraces)
  const openOverview = useTraceStore((state) => state.openOverview)
  const openWorkspace = useTraceStore((state) => state.openWorkspace)
  const selectLoop = useTraceStore((state) => state.selectLoop)

  const loopGroups = useMemo(
    () => buildTraceLoopGroups(traces, turns),
    [traces, turns]
  )
  const visibleLoopGroups = useMemo(
    () => selectVisibleTraceLoopGroups(loopGroups, traceView),
    [loopGroups, traceView]
  )
  const resolvedActiveLoopKey = useMemo(
    () => resolveActiveTraceLoopKey(visibleLoopGroups, activeLoopKey),
    [visibleLoopGroups, activeLoopKey]
  )

  const pageCount = Math.max(1, Math.ceil(totalTraceItems / tracePageSize))

  return (
    <>
      <div className="px-2 pt-2">
        <div className="rounded-lg border border-border/30 bg-muted/20 px-2 py-2">
          <p className="px-1 py-1 text-[13px] font-medium text-foreground/80">
            Trace
          </p>
          <div className="mt-1 space-y-1">
            <button
              type="button"
              onClick={openOverview}
              className={cn(
                "flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors duration-150",
                traceSurface === "overview"
                  ? "bg-muted/65 text-foreground/85"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
              )}
            >
              <Waypoints className="mt-0.5 size-3.5 shrink-0 opacity-70" />
              <span className="min-w-0 text-[12px]">Overview</span>
            </button>
            <button
              type="button"
              onClick={() => openWorkspace("conversation").catch(() => {})}
              className={cn(
                "flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors duration-150",
                traceSurface === "workspace" && traceView === "conversation"
                  ? "bg-muted/65 text-foreground/85"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
              )}
            >
              <Bot className="mt-0.5 size-3.5 shrink-0 opacity-70" />
              <span className="min-w-0 text-[12px]">Conversation</span>
            </button>
            <button
              type="button"
              onClick={() => openWorkspace("compression").catch(() => {})}
              className={cn(
                "flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors duration-150",
                traceSurface === "workspace" && traceView === "compression"
                  ? "bg-muted/65 text-foreground/85"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
              )}
            >
              <Layers3 className="mt-0.5 size-3.5 shrink-0 opacity-70" />
              <span className="min-w-0 text-[12px]">Compression</span>
            </button>
          </div>
        </div>
      </div>

      {traceSurface === "overview" ? (
        <div className="flex-1" />
      ) : (
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
                    ? "bg-muted/65 text-foreground/85"
                    : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
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
                    {formatTraceLoopHeadline(group, { maxLength: 64 })}
                  </span>
                  <span className="mt-1 block text-[11px] text-muted-foreground/80">
                    {formatCompactDateTime(group.latestStartedAtMs)} ·{" "}
                    {formatTraceDuration(group.totalDurationMs)}
                  </span>
                  <span className="mt-0.5 block text-[11px] text-muted-foreground/65">
                    {group.stepCount} llm · {group.toolCount} tool
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      )}

      {traceSurface === "workspace" && pageCount > 1 ? (
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
