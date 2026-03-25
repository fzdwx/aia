import { useEffect, useId, useMemo, useState, type ReactNode } from "react"
import {
  ArrowLeft,
  Bot,
  ChevronDown,
  Loader2,
  RefreshCw,
  Waypoints,
  Wrench,
} from "lucide-react"

import { TraceDetailModal } from "./detail-modal"
import {
  type InspectorTab,
  LlmInspector,
  LoopInspector,
  ToolInspector,
} from "./inspector"
import { TraceOverviewPanel } from "./overview-panel"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { TraceDashboardRange } from "@/lib/types"
import {
  buildTraceLoopGroups,
  formatTraceDuration,
  formatTraceLoopHeadline,
  resolveActiveTraceLoopKey,
  selectVisibleTraceLoopGroups,
  type LoopTimelineNode,
  type TraceLoopGroup,
} from "@/lib/trace-presentation"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { useTraceOverviewStore } from "@/stores/trace-overview-store"
import { useTraceStore } from "@/stores/trace-store"
import { useWorkbenchStore } from "@/stores/workbench-store"

import {
  compactId,
  formatCount,
  formatDateTime,
  formatOverviewRangeLabel,
  loopBadgeVariant,
  loopWindowMs,
  relativeOffsetLabel,
} from "./lib/trace-panel-formatters"
import {
  buildTimelineTreeRows,
  findActiveNode,
  nodeKindLabel,
  nodeSubtitle,
  nodeTitle,
  nodeTone,
} from "./lib/trace-timeline"

const TRACE_OVERVIEW_RANGE_OPTIONS: Array<{
  value: TraceDashboardRange
  label: string
}> = [
  { value: "today", label: "Today" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
]

function TabButton({
  id,
  panelId,
  active,
  children,
  onClick,
}: {
  id: string
  panelId: string
  active: boolean
  children: ReactNode
  onClick: () => void
}) {
  return (
    <button
      id={id}
      type="button"
      role="tab"
      aria-selected={active}
      aria-controls={panelId}
      tabIndex={active ? 0 : -1}
      onClick={onClick}
      className={cn(
        "text-ui-xs min-h-9 border-b-2 px-1 pb-2 font-medium tracking-[0.01em] transition-colors",
        active
          ? "border-foreground text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground"
      )}
    >
      {children}
    </button>
  )
}

function TraceSummaryMetric({
  label,
  value,
}: {
  label: string
  value: ReactNode
}) {
  return (
    <div className="min-w-0">
      <p className="text-meta font-medium text-muted-foreground">{label}</p>
      <div className="text-title mt-1 font-medium text-foreground tabular-nums">
        {value}
      </div>
    </div>
  )
}

function TraceActiveStrip({ group }: { group: TraceLoopGroup }) {
  return (
    <section className="overflow-hidden rounded-xl border border-border/25 bg-background/85">
      <div className="border-b border-border/20 px-4 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-heading-lg font-semibold text-foreground tabular-nums">
                {formatDateTime(group.startedAtMs)}
              </h2>
              <Badge
                variant={loopBadgeVariant(group.finalStatus)}
                className="text-ui-xs"
              >
                {group.finalStatus}
              </Badge>
            </div>
            <p className="text-caption mt-1 truncate text-muted-foreground">
              {formatTraceLoopHeadline(group, {
                compressionLabel: "Context compression log",
                maxLength: 220,
              })}
            </p>
          </div>
          <div className="text-meta text-muted-foreground">
            run {compactId(group.runId, 10, 8)}
          </div>
        </div>
      </div>
      <div className="grid gap-4 px-4 py-3 sm:grid-cols-[minmax(0,1.8fr)_140px_100px_100px]">
        <TraceSummaryMetric
          label="Trace ID"
          value={
            <span className="workspace-code text-foreground">
              {compactId(group.key, 18, 12)}
            </span>
          }
        />
        <TraceSummaryMetric
          label="Duration"
          value={formatTraceDuration(loopWindowMs(group))}
        />
        <TraceSummaryMetric label="Spans" value={group.timeline.length} />
        <TraceSummaryMetric
          label="Issues"
          value={group.failedToolCount > 0 ? group.failedToolCount : "-"}
        />
      </div>
    </section>
  )
}

function WaterfallRow({
  node,
  depth,
  hasChildren,
  groupStartedAtMs,
  selected,
  loading,
  onSelect,
}: {
  node: LoopTimelineNode
  depth: number
  hasChildren: boolean
  groupStartedAtMs: number
  selected: boolean
  loading: boolean
  onSelect: () => void
}) {
  const tone = nodeTone(node)
  const guideWidth = depth * 18 + 18

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      aria-label={`${nodeKindLabel(node)} ${nodeTitle(node)} · ${nodeSubtitle(
        node
      )}`}
      className={cn(
        "w-full border-b border-border/15 px-3 py-3 text-left transition-colors last:border-b-0",
        selected ? "bg-muted/65" : "bg-transparent hover:bg-muted/45"
      )}
    >
      <div
        className="relative flex items-start gap-3"
        style={{ paddingLeft: `${depth * 18}px` }}
      >
        {depth > 0 ? (
          <div
            className="pointer-events-none absolute inset-y-[-10px] left-0"
            style={{ width: `${guideWidth}px` }}
          >
            {Array.from({ length: depth }).map((_, index) => (
              <span
                key={index}
                className="absolute top-0 bottom-0 w-px bg-border/30"
                style={{ left: `${index * 18 + 8}px` }}
              />
            ))}
            <span
              className="absolute top-[18px] h-px w-3 bg-border/30"
              style={{ left: `${(depth - 1) * 18 + 8}px` }}
            />
          </div>
        ) : null}

        <div className="mt-0.5 flex size-4 shrink-0 items-center justify-center text-muted-foreground">
          {hasChildren ? (
            <ChevronDown className="size-3" />
          ) : (
            <span className="size-1.5 rounded-full bg-border/70" />
          )}
        </div>

        <span
          className={cn(
            "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md border",
            tone.dot
          )}
        >
          {node.kind === "agent_root" ? (
            <Waypoints className="size-3" />
          ) : node.kind === "llm_span" ? (
            <Bot className="size-3" />
          ) : (
            <Wrench className="size-3" />
          )}
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-ui truncate font-medium text-foreground">
              {nodeTitle(node)}
            </span>
            <span className="text-caption text-muted-foreground tabular-nums">
              {formatTraceDuration(node.durationMs)}
            </span>
            <span
              className={cn(
                "text-ui-xs rounded-sm border px-1.5 py-0.5 font-medium uppercase",
                tone.badge
              )}
            >
              {nodeKindLabel(node)}
            </span>
            {node.kind !== "agent_root" &&
            "status" in node &&
            node.status === "error" ? (
              <span className="text-ui-xs rounded-sm border border-destructive/30 bg-destructive/[0.08] px-1.5 py-0.5 font-medium text-destructive uppercase">
                err
              </span>
            ) : null}
            {loading ? (
              <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
            ) : null}
          </div>
          <div className="text-meta mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-muted-foreground">
            <span>{nodeSubtitle(node)}</span>
            <span>·</span>
            <span className="tabular-nums">
              {relativeOffsetLabel(node.startedAtMs, groupStartedAtMs)}
            </span>
          </div>
        </div>

        <div className="shrink-0 pt-0.5 text-right">
          <div className="workspace-code text-foreground">
            {node.kind === "llm_span"
              ? `${node.toolCount} tool`
              : node.kind === "tool_span"
                ? "internal"
                : "root"}
          </div>
        </div>
      </div>
    </button>
  )
}

export function TracePanel() {
  const turns = useChatStore((state) => state.turns)
  const setView = useWorkbenchStore((state) => state.setView)
  const traces = useTraceStore((state) => state.traces)
  const traceSurface = useTraceStore((state) => state.traceSurface)
  const traceView = useTraceStore((state) => state.traceView)
  const activeLoopKey = useTraceStore((state) => state.activeLoopKey)
  const selectedNodeId = useTraceStore((state) => state.selectedNodeId)
  const selectedTraceId = useTraceStore((state) => state.selectedTraceId)
  const selectedTrace = useTraceStore((state) => state.selectedTrace)
  const selectedLoop = useTraceStore((state) => state.selectedLoop)
  const traceLoading = useTraceStore((state) => state.traceLoading)
  const traceError = useTraceStore((state) => state.traceError)
  const tracePage = useTraceStore((state) => state.tracePage)
  const refreshTraces = useTraceStore((state) => state.refreshTraces)
  const selectTrace = useTraceStore((state) => state.selectTrace)
  const selectNode = useTraceStore((state) => state.selectNode)
  const overviewRange = useTraceOverviewStore((state) => state.range)
  const overviewDashboard = useTraceOverviewStore((state) => state.dashboard)
  const refreshOverview = useTraceOverviewStore((state) => state.refresh)
  const setOverviewRange = useTraceOverviewStore((state) => state.setRange)

  const [payloadOpen, setPayloadOpen] = useState(false)
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("content")
  const inspectorId = useId()

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

  const activeGroup = useMemo(
    () =>
      visibleLoopGroups.find((group) => group.key === resolvedActiveLoopKey) ??
      visibleLoopGroups[0] ??
      null,
    [visibleLoopGroups, resolvedActiveLoopKey]
  )

  const resolvedSelectedNodeId =
    selectedNodeId &&
    activeGroup?.timeline.some((node) => node.id === selectedNodeId)
      ? selectedNodeId
      : (activeGroup?.finalSpanId ?? activeGroup?.timeline[0]?.id ?? null)

  const activeNode = useMemo(
    () => findActiveNode(activeGroup, resolvedSelectedNodeId),
    [activeGroup, resolvedSelectedNodeId]
  )
  const treeRows = useMemo(
    () => (activeGroup ? buildTimelineTreeRows(activeGroup) : []),
    [activeGroup]
  )

  useEffect(() => {
    if (traceSurface !== "workspace") return
    setInspectorTab("content")
  }, [activeNode?.id, traceSurface])

  useEffect(() => {
    if (traceSurface !== "workspace") return
    if (!activeNode || activeNode.kind === "agent_root") return
    if (selectedTraceId === activeNode.trace.id) return
    selectTrace(activeNode.trace.id).catch(() => {})
  }, [activeNode, selectTrace, selectedTraceId, traceSurface])

  const inspectedTrace = useMemo(
    () =>
      activeNode?.kind === "agent_root"
        ? null
        : (selectedLoop?.trace_details.find(
            (trace) => trace.id === activeNode?.trace.id
          ) ??
          (selectedTrace?.id === activeNode?.trace.id ? selectedTrace : null)),
    [activeNode, selectedLoop?.trace_details, selectedTrace]
  )

  const traceDescription =
    traceView === "compression"
      ? "Review compression runs, loop timing, and payload details without mixing them into the main conversation trace stream."
      : "Inspect conversation loops, waterfall timing, and span payloads from the current workspace."
  const overviewSummaryMetrics = overviewDashboard
    ? [
        {
          label: "requests",
          value: formatCount(overviewDashboard.overall_summary.total_requests),
        },
        {
          label: "sessions",
          value: formatCount(overviewDashboard.current.total_sessions),
        },
        {
          label: "models",
          value: formatCount(overviewDashboard.overall_summary.unique_models),
        },
        {
          label: "LLM spans",
          value: formatCount(overviewDashboard.overall_summary.total_llm_spans),
        },
        {
          label: "tool spans",
          value: formatCount(
            overviewDashboard.overall_summary.total_tool_spans
          ),
        },
        {
          label: "tool req",
          value: formatCount(
            overviewDashboard.overall_summary.requests_with_tools
          ),
        },
      ]
    : []

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="border-b border-border/30 px-3.5 py-1.5">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div className="flex min-w-0 flex-1 items-start gap-2.5">
            <button
              type="button"
              onClick={() => setView("chat")}
              className="mt-0.5 flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
              aria-label="Back to chat"
            >
              <ArrowLeft className="size-3" />
            </button>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-1.5">
                <h1 className="workspace-panel-title">Trace</h1>
                <Badge variant="secondary" className="text-ui-xs">
                  {traceSurface === "overview" ? "overview" : traceView}
                </Badge>
              </div>
              {traceSurface === "overview" ? (
                <div className="text-meta mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-muted-foreground">
                  <span className="rounded-full border border-border/16 bg-background/60 px-2 py-0.5">
                    {formatOverviewRangeLabel(overviewRange)}
                  </span>
                  {overviewSummaryMetrics.map((metric) => (
                    <span
                      key={metric.label}
                      className="rounded-full border border-border/16 bg-background/45 px-2 py-0.5"
                    >
                      <span className="text-foreground tabular-nums">
                        {metric.value}
                      </span>{" "}
                      {metric.label}
                    </span>
                  ))}
                </div>
              ) : traceDescription ? (
                <p className="workspace-panel-copy mt-0.5 text-muted-foreground">
                  {traceDescription}
                </p>
              ) : null}
            </div>
          </div>

          <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
            {traceSurface === "overview" ? (
              <div
                role="group"
                aria-label="Trace overview range"
                className="inline-flex rounded-full border border-border/40 bg-background/85 p-0.5"
              >
                {TRACE_OVERVIEW_RANGE_OPTIONS.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    onClick={() =>
                      void setOverviewRange(option.value).catch(() => {})
                    }
                    aria-pressed={option.value === overviewRange}
                    className={cn(
                      "text-ui-xs min-h-7 rounded-full px-2.5 py-1 font-medium tracking-[0.01em] transition-colors",
                      option.value === overviewRange
                        ? "bg-foreground text-background"
                        : "text-muted-foreground hover:text-foreground"
                    )}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            ) : null}
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                traceSurface === "overview"
                  ? refreshOverview().catch(() => {})
                  : refreshTraces({ page: tracePage, view: traceView })
              }
              className="text-ui-xs h-7 px-2.5 tracking-[0.07em]"
            >
              <RefreshCw className="size-3" />
              Refresh
            </Button>
          </div>
        </div>
      </div>

      {traceSurface === "overview" ? (
        <TraceOverviewPanel />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-3">
          <div className="mx-auto flex min-h-0 w-full max-w-[1440px] flex-1 flex-col gap-2">
            {traceError ? (
              <div className="text-caption shrink-0 rounded-xl border border-destructive/25 bg-destructive/[0.05] px-4 py-3 text-destructive">
                {traceError}
              </div>
            ) : null}

            {visibleLoopGroups.length === 0 && !traceLoading ? (
              <section className="flex min-h-0 flex-1 flex-col items-center justify-center rounded-2xl border border-border/30 bg-card/70 px-6 py-16 text-center shadow-[var(--workspace-shadow)]">
                <Waypoints className="size-10 text-muted-foreground/30" />
                <p className="workspace-panel-title mt-4 text-foreground/70">
                  {traceView === "compression"
                    ? "No compression logs yet"
                    : "No traces yet"}
                </p>
                <p className="workspace-panel-copy mx-auto mt-1 text-muted-foreground">
                  {traceView === "compression"
                    ? "Trigger context compression to inspect compression calls and summaries here."
                    : "Start a conversation to see agent loops and LLM spans here."}
                </p>
              </section>
            ) : null}

            {activeGroup ? (
              <div className="shrink-0">
                <TraceActiveStrip group={activeGroup} />
              </div>
            ) : null}

            {visibleLoopGroups.length > 0 ? (
              <div className="grid min-h-0 flex-1 overflow-hidden rounded-xl border border-border/25 bg-background/80 xl:grid-cols-[minmax(0,1.02fr)_minmax(360px,0.98fr)]">
                <div className="flex min-h-0 flex-col overflow-hidden">
                  <div className="shrink-0 border-b border-border/20 bg-muted/[0.08] px-3 py-2.5">
                    <div className="flex items-center justify-between gap-2">
                      <p className="workspace-section-label text-muted-foreground">
                        Waterfall
                      </p>
                      {activeGroup ? (
                        <span className="text-meta text-muted-foreground">
                          {treeRows.length} spans
                        </span>
                      ) : null}
                    </div>
                  </div>

                  <div className="min-h-0 flex-1 overflow-y-auto">
                    {activeGroup ? (
                      <div>
                        {treeRows.map(({ node, depth, hasChildren }) => (
                          <WaterfallRow
                            key={node.id}
                            node={node}
                            depth={depth}
                            hasChildren={hasChildren}
                            groupStartedAtMs={activeGroup.startedAtMs}
                            selected={node.id === activeNode?.id}
                            loading={
                              node.kind !== "agent_root" &&
                              selectedTraceId === node.trace.id &&
                              traceLoading
                            }
                            onSelect={() => selectNode(node.id)}
                          />
                        ))}
                      </div>
                    ) : null}
                  </div>
                </div>

                <div className="flex min-h-0 flex-col overflow-hidden border-l border-border/20 bg-background">
                  <div className="shrink-0 border-b border-border/20">
                    <div className="px-4 py-3">
                      {activeNode ? (
                        <div className="min-w-0">
                          <div className="text-ui flex flex-wrap items-center gap-2 text-muted-foreground">
                            <span
                              className={cn(
                                "text-ui-xs inline-flex items-center rounded-sm border px-1.5 py-0.5 font-medium uppercase",
                                nodeTone(activeNode).badge
                              )}
                            >
                              {nodeKindLabel(activeNode)}
                            </span>
                          </div>
                          <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
                            <h3 className="text-body-sm font-semibold text-foreground">
                              {nodeTitle(activeNode)}
                            </h3>
                            <span className="text-caption text-muted-foreground tabular-nums">
                              {formatTraceDuration(activeNode.durationMs)}
                            </span>
                            <span className="text-caption text-muted-foreground">
                              {formatDateTime(activeNode.startedAtMs)}
                            </span>
                          </div>
                          <p className="text-caption mt-1 text-muted-foreground">
                            {nodeSubtitle(activeNode)}
                          </p>
                        </div>
                      ) : (
                        <p className="text-ui text-muted-foreground">
                          Select a span.
                        </p>
                      )}
                    </div>

                    <div
                      role="tablist"
                      aria-label="Trace inspector sections"
                      className="flex items-center gap-5 px-4"
                    >
                      <TabButton
                        id={`${inspectorId}-overview-tab`}
                        panelId={`${inspectorId}-panel`}
                        active={inspectorTab === "overview"}
                        onClick={() => setInspectorTab("overview")}
                      >
                        Attributes
                      </TabButton>
                      <TabButton
                        id={`${inspectorId}-content-tab`}
                        panelId={`${inspectorId}-panel`}
                        active={inspectorTab === "content"}
                        onClick={() => setInspectorTab("content")}
                      >
                        Input/output
                      </TabButton>
                      <TabButton
                        id={`${inspectorId}-events-tab`}
                        panelId={`${inspectorId}-panel`}
                        active={inspectorTab === "events"}
                        onClick={() => setInspectorTab("events")}
                      >
                        Events
                      </TabButton>
                    </div>
                  </div>

                  <div
                    id={`${inspectorId}-panel`}
                    role="tabpanel"
                    aria-labelledby={`${inspectorId}-${inspectorTab}-tab`}
                    className="min-h-0 flex-1 overflow-y-auto bg-muted/[0.04] p-4"
                  >
                    {activeGroup && activeNode ? (
                      activeNode.kind === "agent_root" ? (
                        <LoopInspector group={activeGroup} tab={inspectorTab} />
                      ) : activeNode.kind === "llm_span" ? (
                        <LlmInspector
                          group={activeGroup}
                          node={activeNode}
                          trace={inspectedTrace}
                          tab={inspectorTab}
                          loading={traceLoading && inspectedTrace == null}
                          onOpenPayload={() => setPayloadOpen(true)}
                        />
                      ) : (
                        <ToolInspector
                          node={activeNode}
                          trace={inspectedTrace}
                          tab={inspectorTab}
                        />
                      )
                    ) : (
                      <div className="workspace-panel-copy flex h-full items-center justify-center text-muted-foreground">
                        Select a span.
                      </div>
                    )}
                  </div>
                </div>
              </div>
            ) : null}
          </div>
        </div>
      )}

      <TraceDetailModal
        open={payloadOpen && selectedTraceId != null}
        trace={selectedTrace}
        loading={traceLoading}
        onOpenChange={setPayloadOpen}
      />
    </div>
  )
}
