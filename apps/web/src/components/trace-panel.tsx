import { useEffect, useId, useMemo, useState, type ReactNode } from "react"
import {
  ArrowLeft,
  Bot,
  ChevronDown,
  ExternalLink,
  Loader2,
  RefreshCw,
  Waypoints,
  Wrench,
} from "lucide-react"

import { TraceDetailModal } from "@/components/trace-detail-modal"
import { TraceOverviewPanel } from "@/components/trace-overview-panel"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  asArray,
  asRecord,
  asString,
  extractTraceText,
} from "@/lib/trace-inspection"
import type { TraceDashboardRange, TraceEvent, TraceRecord } from "@/lib/types"
import {
  buildTraceLoopGroups,
  formatTraceDuration,
  formatTraceLoopHeadline,
  resolveActiveTraceLoopKey,
  selectVisibleTraceLoopGroups,
  type LoopTimelineNode,
  type TraceLoopGroup,
} from "@/lib/trace-presentation"
import { getToolDisplayName } from "@/lib/tool-display"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { useTraceOverviewStore } from "@/stores/trace-overview-store"
import { useTraceStore } from "@/stores/trace-store"

type InspectorTab = "content" | "overview" | "events"

const TRACE_OVERVIEW_RANGE_OPTIONS: Array<{
  value: TraceDashboardRange
  label: string
}> = [
  { value: "today", label: "Today" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
]

function formatOverviewRangeLabel(range: TraceDashboardRange) {
  switch (range) {
    case "today":
      return "today"
    case "week":
      return "the last 7 days"
    case "month":
      return "the last 30 days"
  }
}

function formatDateTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  })
}

function formatCount(value: number | null | undefined) {
  return value != null ? value.toLocaleString("en-US") : "-"
}

function truncate(text: string, max: number) {
  if (text.length <= max) return text
  return `${text.slice(0, max - 1)}...`
}

function compactId(value: string, head = 8, tail = 6) {
  if (value.length <= head + tail + 1) return value
  return `${value.slice(0, head)}...${value.slice(-tail)}`
}

function asEventRecord(value: unknown): Record<string, unknown> | null {
  return asRecord(value)
}

function loopBadgeVariant(status: TraceLoopGroup["finalStatus"]) {
  switch (status) {
    case "failed":
      return "destructive" as const
    case "partial":
      return "outline" as const
    default:
      return "secondary" as const
  }
}

function findActiveNode(
  group: TraceLoopGroup | null,
  selectedNodeId: string | null
) {
  if (!group) return null
  if (selectedNodeId) {
    const matched = group.timeline.find((node) => node.id === selectedNodeId)
    if (matched) return matched
  }
  return group.timeline[group.timeline.length - 1] ?? null
}

function collectSystemPrompts(trace: TraceRecord | null) {
  if (!trace) return []

  const request = asRecord(trace.provider_request)
  if (!request) return []

  const prompts: string[] = []
  const instructions = asString(request.instructions)
  if (instructions) {
    prompts.push(instructions)
  }

  const messages = asArray(request.messages)
  for (const item of messages) {
    const record = asRecord(item)
    if (record?.role === "system") {
      const content = extractTraceText(record.content)
      if (content) prompts.push(content)
    }
  }

  const input = asArray(request.input)
  for (const item of input) {
    const record = asRecord(item)
    if (record?.role === "system") {
      const content = extractTraceText(record.content)
      if (content) prompts.push(content)
    }
  }

  return prompts
}

function collectToolNames(trace: TraceRecord | null) {
  if (!trace) return []

  const requestSummary = asRecord(trace.request_summary)
  const explicit = asArray(requestSummary?.tool_names).filter(
    (value): value is string => typeof value === "string"
  )
  if (explicit.length > 0) return explicit

  const request = asRecord(trace.provider_request)
  return asArray(request?.tools)
    .map((tool) => {
      const record = asRecord(tool)
      const fn = asRecord(record?.function)
      return asString(record?.name) ?? asString(fn?.name)
    })
    .filter((value): value is string => Boolean(value))
}

function collectAssistantPreview(trace: TraceRecord | null) {
  if (!trace) return null

  const summary = asRecord(trace.response_summary)
  const assistantText =
    asString(summary?.assistant_text) ||
    extractTraceText(summary?.assistant_text)

  if (assistantText) return assistantText

  return extractAssistantPreviewFromResponseBody(trace.response_body)
}

function extractAssistantPreviewFromResponseBody(body: string | null) {
  if (!body) return null

  const parsedSse = extractAssistantPreviewFromSseBody(body)
  if (parsedSse) return parsedSse

  const parsedJson = extractAssistantPreviewFromJsonBody(body)
  if (parsedJson) return parsedJson

  return null
}

function extractAssistantPreviewFromJsonBody(body: string) {
  try {
    const payload = JSON.parse(body)
    const texts = collectAssistantTextsFromPayload(payload)
    return texts.length > 0 ? texts.join("\n") : null
  } catch {
    return null
  }
}

function extractAssistantPreviewFromSseBody(body: string) {
  const lines = body.split("\n")
  const outputChunks: string[] = []
  const completedPayloadTexts: string[] = []

  for (const line of lines) {
    const data = line.startsWith("data: ") ? line.slice(6) : null
    if (!data || data === "[DONE]") continue

    try {
      const event = JSON.parse(data)
      const type = asString(asRecord(event)?.type)

      if (type === "response.output_text.delta") {
        const text = extractTraceText(asRecord(event)?.delta)
        if (text) outputChunks.push(text)
        continue
      }

      if (type === "response.output_text.done" && outputChunks.length === 0) {
        const text = extractTraceText(asRecord(event)?.text)
        if (text) outputChunks.push(text)
        continue
      }

      if (type === "response.completed") {
        const response = asRecord(event)?.response
        const texts = collectAssistantTextsFromPayload(response)
        if (texts.length > 0) {
          completedPayloadTexts.push(...texts)
        }
      }
    } catch {
      continue
    }
  }

  if (outputChunks.length > 0) {
    return outputChunks.join("").trim() || null
  }

  if (completedPayloadTexts.length > 0) {
    return completedPayloadTexts.join("\n").trim() || null
  }

  return null
}

function collectAssistantTextsFromPayload(payload: unknown): string[] {
  const record = asRecord(payload)
  if (!record) return []

  const texts: string[] = []
  const push = (value: unknown) => {
    const text = extractTraceText(value).trim()
    if (text && !texts.includes(text)) {
      texts.push(text)
    }
  }

  const choices = asArray(record.choices)
  for (const choice of choices) {
    const message = asRecord(asRecord(choice)?.message)
    if (!message) continue
    push(message.content)
  }

  const output = asArray(record.output)
  for (const item of output) {
    const outputItem = asRecord(item)
    if (!outputItem) continue
    if (
      asString(outputItem.role) &&
      asString(outputItem.role) !== "assistant"
    ) {
      continue
    }

    if (asString(outputItem.type) === "message") {
      push(outputItem.content)
      continue
    }

    push(outputItem.content)
  }

  return texts
}

function loopWindowMs(group: TraceLoopGroup) {
  const explicit =
    group.finishedAtMs != null ? group.finishedAtMs - group.startedAtMs : null
  return Math.max(1, explicit ?? group.totalDurationMs ?? 1)
}

function relativeOffsetLabel(
  nodeStartedAtMs: number,
  groupStartedAtMs: number
) {
  const delta = Math.max(0, nodeStartedAtMs - groupStartedAtMs)
  return `+${formatTraceDuration(delta)}`
}

function nodeTitle(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return "invoke_agent"
    case "llm_span":
      return node.operationName
    case "tool_span":
      return getToolDisplayName(node.trace.model)
  }
}

function nodeSubtitle(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return "root span"
    case "llm_span":
      return node.trace.total_tokens != null && node.trace.total_tokens > 0
        ? `${node.trace.model} · ${formatCount(node.trace.total_tokens)} tok`
        : node.trace.model
    case "tool_span":
      return node.trace.endpoint_path.startsWith("/tools/")
        ? node.operationName
        : node.trace.endpoint_path
  }
}

function nodeKindLabel(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return "Agent"
    case "llm_span":
      return "LLM"
    case "tool_span":
      return "Tool"
  }
}

function nodeSpanId(node: LoopTimelineNode) {
  return node.kind === "agent_root" ? node.id : node.trace.span_id
}

type TimelineTreeRow = {
  node: LoopTimelineNode
  depth: number
  hasChildren: boolean
}

function buildTimelineTreeRows(group: TraceLoopGroup): TimelineTreeRow[] {
  if (group.timeline.length === 0) return []

  const spanIds = new Set(group.timeline.map((node) => nodeSpanId(node)))
  const root = group.timeline[0]
  const rootSpanId = nodeSpanId(root)
  const childrenByParent = new Map<string | null, LoopTimelineNode[]>()

  const pushChild = (parentId: string | null, node: LoopTimelineNode) => {
    const current = childrenByParent.get(parentId) ?? []
    current.push(node)
    childrenByParent.set(parentId, current)
  }

  for (const node of group.timeline) {
    if (node.kind === "agent_root") {
      pushChild(null, node)
      continue
    }

    const parentSpanId =
      node.trace.parent_span_id && spanIds.has(node.trace.parent_span_id)
        ? node.trace.parent_span_id
        : rootSpanId
    pushChild(parentSpanId, node)
  }

  for (const [, children] of childrenByParent) {
    children.sort((left, right) => {
      if (left.startedAtMs !== right.startedAtMs) {
        return left.startedAtMs - right.startedAtMs
      }
      return left.id.localeCompare(right.id)
    })
  }

  const rows: TimelineTreeRow[] = []
  const visit = (node: LoopTimelineNode, depth: number) => {
    const children = childrenByParent.get(nodeSpanId(node)) ?? []
    rows.push({
      node,
      depth,
      hasChildren: children.length > 0,
    })
    for (const child of children) {
      visit(child, depth + 1)
    }
  }

  for (const node of childrenByParent.get(null) ?? []) {
    visit(node, 0)
  }

  return rows
}

function nodeTone(node: LoopTimelineNode) {
  if (node.kind === "agent_root") {
    return {
      frame: "trace-tone-agent-frame",
      dot: "trace-tone-agent-dot",
      badge: "trace-tone-agent-badge",
    }
  }

  if (node.kind === "tool_span") {
    return node.status === "error"
      ? {
          frame: "border-destructive/25 bg-destructive/[0.04]",
          dot: "border-destructive/30 bg-destructive/15 text-destructive",
          badge: "border-destructive/30 bg-destructive/[0.08] text-destructive",
        }
      : {
          frame: "trace-tone-tool-frame",
          dot: "trace-tone-tool-dot",
          badge: "trace-tone-tool-badge",
        }
  }

  return node.status === "error"
    ? {
        frame: "border-destructive/25 bg-destructive/[0.04]",
        dot: "border-destructive/30 bg-destructive/15 text-destructive",
        badge: "border-destructive/30 bg-destructive/[0.08] text-destructive",
      }
    : {
        frame: "border-border/40 bg-background/80",
        dot: "border-border/45 bg-muted/35 text-foreground/80",
        badge: "border-border/25 bg-muted/50 text-muted-foreground",
      }
}

function summarizeEvent(event: TraceEvent) {
  const attributes = asEventRecord(event.attributes)
  switch (event.name) {
    case "response.first_text_delta":
    case "response.first_reasoning_delta":
      return asString(attributes?.preview) ?? null
    case "response.tool_call_detected":
    case "response.tool_call_started":
      return asString(attributes?.tool_name) ?? null
    case "response.completed":
      return asString(attributes?.stop_reason) ?? null
    case "response.failed":
      return asString(attributes?.error) ?? null
    default:
      return null
  }
}

function buildRootEvents(group: TraceLoopGroup) {
  const events: Array<{
    key: string
    name: string
    at_ms: number
    summary?: string | null
    attributes?: Record<string, unknown> | null
  }> = [
    {
      key: `${group.key}:root:start`,
      name: "loop.started",
      at_ms: group.startedAtMs,
      summary: group.userMessage ? truncate(group.userMessage, 120) : null,
      attributes: {
        turn_id: group.turnId,
        run_id: group.runId,
      },
    },
  ]

  if (group.finishedAtMs != null) {
    events.push({
      key: `${group.key}:root:end`,
      name: group.finalStatus === "failed" ? "loop.failed" : "loop.completed",
      at_ms: group.finishedAtMs,
      summary: `${group.stepCount} llm spans · ${group.toolCount} tool spans`,
      attributes: {
        status: group.finalStatus,
        llm_spans: group.stepCount,
        tool_spans: group.toolCount,
        total_tokens: group.totalTokens,
      },
    })
  }

  return events
}

function buildToolEvents(
  node: Extract<LoopTimelineNode, { kind: "tool_span" }>
) {
  const outcome =
    node.trace.status === "failed" ? "tool.failed" : "tool.completed"

  return [
    {
      key: `${node.id}:start`,
      name: "tool.started",
      at_ms: node.startedAtMs,
      summary: node.trace.model,
      attributes: {
        span_id: node.trace.span_id,
        tool_name: getToolDisplayName(node.trace.model),
      },
    },
    {
      key: `${node.id}:end`,
      name: outcome,
      at_ms: node.finishedAtMs ?? node.startedAtMs,
      summary: node.trace.error
        ? truncate(node.trace.error, 120)
        : (node.trace.stop_reason ?? null),
      attributes: {
        span_id: node.trace.span_id,
        tool_name: getToolDisplayName(node.trace.model),
        error: node.trace.error,
      },
    },
  ]
}

function DetailList({
  items,
}: {
  items: Array<{ label: string; value: ReactNode }>
}) {
  return (
    <dl className="divide-y divide-border/20 overflow-hidden rounded-xl border border-border/20 bg-background">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex flex-wrap items-start justify-between gap-x-3 gap-y-1 px-3 py-2"
        >
          <dt className="workspace-section-label text-muted-foreground">
            {item.label}
          </dt>
          <dd className="text-caption max-w-[70%] min-w-0 text-right text-foreground">
            {item.value}
          </dd>
        </div>
      ))}
    </dl>
  )
}

function Section({
  title,
  action,
  children,
}: {
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="overflow-hidden rounded-xl border border-border/20 bg-background/85">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/20 bg-muted/[0.12] px-3 py-2.5">
        <h3 className="workspace-section-label text-muted-foreground">
          {title}
        </h3>
        {action}
      </div>
      <div className="space-y-3 p-3">{children}</div>
    </section>
  )
}

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
        "text-caption min-h-10 border-b-2 px-1 pb-2 font-medium transition-colors",
        active
          ? "border-foreground text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground"
      )}
    >
      {children}
    </button>
  )
}

function FieldBlock({
  label,
  children,
}: {
  label: string
  children: ReactNode
}) {
  return (
    <div className="space-y-1.5">
      <p className="workspace-section-label text-muted-foreground">{label}</p>
      {children}
    </div>
  )
}

function TextBlock({
  value,
  className,
}: {
  value: string | null
  className?: string
}) {
  if (!value) {
    return <p className="text-caption text-muted-foreground">-</p>
  }

  return (
    <pre
      className={cn(
        "text-caption overflow-x-auto rounded-lg border border-border/20 bg-background px-3 py-2.5 whitespace-pre-wrap text-foreground",
        className
      )}
    >
      {value}
    </pre>
  )
}

function formatScalar(value: unknown): string {
  if (typeof value === "string") return value
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value)
  }
  if (value == null) return "-"
  return JSON.stringify(value)
}

function filterToolArgumentsForInspector(
  value: unknown
): Record<string, unknown> {
  const record = asRecord(value)
  if (!record) return {}
  return record
}

function StructuredArguments({ value }: { value: unknown }) {
  const record = asRecord(value)

  if (!record || Object.keys(record).length === 0) {
    return <p className="text-caption text-muted-foreground">No arguments.</p>
  }

  const scalarEntries = Object.entries(record).filter(([, entry]) => {
    return (
      entry == null ||
      typeof entry === "string" ||
      typeof entry === "number" ||
      typeof entry === "boolean"
    )
  })
  const nestedEntries = Object.entries(record).filter(([, entry]) => {
    return !(
      entry == null ||
      typeof entry === "string" ||
      typeof entry === "number" ||
      typeof entry === "boolean"
    )
  })

  return (
    <div className="space-y-2">
      {scalarEntries.length > 0 ? (
        <DetailList
          items={scalarEntries.map(([label, entry]) => ({
            label,
            value: formatScalar(entry),
          }))}
        />
      ) : null}
      {nestedEntries.map(([label, entry]) => (
        <RawJson key={label} title={label} value={entry} />
      ))}
    </div>
  )
}

function ExpandableTextBlock({
  value,
  tone = "default",
}: {
  value: string | null
  tone?: "default" | "danger"
}) {
  const [open, setOpen] = useState(false)

  if (!value) {
    return <p className="text-caption text-muted-foreground">-</p>
  }

  const needsCollapse = value.length > 320 || value.split("\n").length > 10

  return (
    <div className="space-y-2">
      <pre
        className={cn(
          "text-caption overflow-auto rounded-lg border border-border/20 bg-background px-3 py-2.5 whitespace-pre-wrap text-foreground",
          !open && needsCollapse && "max-h-48",
          tone === "danger" &&
            "border-destructive/20 bg-destructive/[0.04] text-destructive"
        )}
      >
        {value}
      </pre>
      {needsCollapse ? (
        <button
          onClick={() => setOpen((current) => !current)}
          className="text-meta font-medium text-muted-foreground transition-colors hover:text-foreground"
        >
          {open ? "Collapse" : "Expand"}
        </button>
      ) : null}
    </div>
  )
}

function RawJson({ title, value }: { title: string; value: unknown }) {
  return (
    <Collapsible className="overflow-hidden rounded-xl border border-border/20 bg-background">
      <CollapsibleTrigger className="flex w-full items-center justify-between border-b border-border/20 bg-muted/[0.12] px-3 py-2.5 text-left">
        <span className="workspace-section-label text-muted-foreground">
          {title}
        </span>
        <span className="text-meta text-muted-foreground">JSON</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="p-3">
        <pre className="text-meta overflow-x-auto rounded-lg border border-border/20 bg-background px-3 py-2.5 text-foreground">
          {JSON.stringify(value, null, 2)}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  )
}

function EventTimeline({
  events,
  emptyLabel,
}: {
  events: Array<{
    key: string
    name: string
    at_ms: number
    summary?: string | null
    attributes?: Record<string, unknown> | null
  }>
  emptyLabel: string
}) {
  if (events.length === 0) {
    return <p className="text-caption text-muted-foreground">{emptyLabel}</p>
  }

  return (
    <div className="relative space-y-1 pl-4 before:absolute before:top-2 before:bottom-2 before:left-[7px] before:w-px before:bg-border/25">
      {events.map((event) => (
        <Collapsible
          key={event.key}
          className="relative rounded-xl border border-border/20 bg-background before:absolute before:top-3.5 before:-left-[12px] before:size-1.5 before:rounded-full before:bg-foreground/35"
        >
          <CollapsibleTrigger className="flex w-full items-start justify-between gap-3 px-3 py-2.5 text-left">
            <div className="min-w-0 space-y-0.5">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-ui font-medium text-foreground">
                  {event.name}
                </span>
                <span className="text-meta text-muted-foreground">
                  {formatDateTime(event.at_ms)}
                </span>
              </div>
              {event.summary ? (
                <p className="text-meta leading-4 text-muted-foreground">
                  {event.summary}
                </p>
              ) : null}
            </div>
          </CollapsibleTrigger>
          {event.attributes && Object.keys(event.attributes).length > 0 ? (
            <CollapsibleContent className="border-t border-border/20 px-3 py-3">
              <pre className="text-meta overflow-x-auto rounded-lg border border-border/20 bg-background px-3 py-2.5 text-foreground">
                {JSON.stringify(event.attributes, null, 2)}
              </pre>
            </CollapsibleContent>
          ) : null}
        </Collapsible>
      ))}
    </div>
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

function LoopInspector({
  group,
  tab,
}: {
  group: TraceLoopGroup
  tab: InspectorTab
}) {
  if (tab === "content") {
    return (
      <div className="space-y-3">
        <Section title="Input">
          <FieldBlock label="User message">
            <TextBlock value={group.userMessage} className="bg-background/80" />
          </FieldBlock>
        </Section>
        <Section title="Output">
          <FieldBlock label="Assistant reply">
            <TextBlock
              value={group.assistantMessage}
              className="trace-accent-surface"
            />
          </FieldBlock>
        </Section>
      </div>
    )
  }

  if (tab === "events") {
    return (
      <Section title="Events">
        <EventTimeline
          events={buildRootEvents(group)}
          emptyLabel="No root events."
        />
      </Section>
    )
  }

  return (
    <div className="space-y-3">
      <Section title="Attributes">
        <DetailList
          items={[
            { label: "status", value: group.finalStatus },
            {
              label: "window",
              value: formatTraceDuration(loopWindowMs(group)),
            },
            { label: "started", value: formatDateTime(group.startedAtMs) },
            { label: "turn", value: group.turnId },
            { label: "llm spans", value: String(group.stepCount) },
            { label: "tool spans", value: String(group.toolCount) },
          ]}
        />
      </Section>

      <Section title="Trace fields">
        <DetailList
          items={[
            { label: "trace id", value: group.key },
            { label: "run id", value: group.runId },
            { label: "root span", value: group.timeline[0]?.id ?? "-" },
          ]}
        />
      </Section>
    </div>
  )
}

function LlmInspector({
  group,
  node,
  trace,
  tab,
  loading,
  onOpenPayload,
}: {
  group: TraceLoopGroup
  node: Extract<LoopTimelineNode, { kind: "llm_span" }>
  trace: TraceRecord | null
  tab: InspectorTab
  loading: boolean
  onOpenPayload: () => void
}) {
  const systemPrompts = useMemo(() => collectSystemPrompts(trace), [trace])
  const toolNames = useMemo(() => collectToolNames(trace), [trace])
  const assistantPreview = useMemo(
    () => collectAssistantPreview(trace),
    [trace]
  )

  if (tab === "content") {
    return (
      <div className="space-y-3">
        <Section
          title="Input"
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={onOpenPayload}
              disabled={loading}
            >
              {loading ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <ExternalLink className="size-3.5" />
              )}
              Payload
            </Button>
          }
        >
          <div className="space-y-3">
            <FieldBlock label="System prompts">
              {systemPrompts.length === 0 ? (
                <p className="text-caption text-muted-foreground">-</p>
              ) : (
                <div className="space-y-2">
                  {systemPrompts.map((prompt, index) => (
                    <TextBlock
                      key={`${index}-${prompt.slice(0, 24)}`}
                      value={prompt}
                      className="trace-accent-surface"
                    />
                  ))}
                </div>
              )}
            </FieldBlock>

            <FieldBlock label="User message">
              <TextBlock value={group.userMessage} />
            </FieldBlock>

            <FieldBlock label="Enabled tools">
              {toolNames.length === 0 ? (
                <p className="text-caption text-muted-foreground">-</p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {toolNames.map((tool) => (
                    <Badge
                      key={tool}
                      variant="secondary"
                      className="text-ui-xs"
                    >
                      {tool}
                    </Badge>
                  ))}
                </div>
              )}
            </FieldBlock>
          </div>
        </Section>

        <Section title="Output">
          <FieldBlock label="Assistant preview">
            <TextBlock value={assistantPreview} />
          </FieldBlock>
          {node.trace.error ? (
            <FieldBlock label="Error">
              <TextBlock
                value={node.trace.error}
                className="border-destructive/20 bg-destructive/[0.04] text-destructive"
              />
            </FieldBlock>
          ) : null}
        </Section>
      </div>
    )
  }

  if (tab === "events") {
    return (
      <Section title="Events">
        <EventTimeline
          events={(trace?.events ?? []).map((event, index) => ({
            key: `${event.name}-${event.at_ms}-${index}`,
            name: event.name,
            at_ms: event.at_ms,
            summary: summarizeEvent(event),
            attributes: event.attributes,
          }))}
          emptyLabel="No LLM events captured."
        />
      </Section>
    )
  }

  return (
    <div className="space-y-3">
      <Section title="Attributes">
        <DetailList
          items={[
            { label: "status", value: node.status },
            { label: "model", value: node.trace.model },
            { label: "provider", value: node.trace.provider },
            { label: "operation", value: node.operationName },
            {
              label: "duration",
              value: formatTraceDuration(node.trace.duration_ms),
            },
            {
              label: "tokens",
              value: formatCount(node.trace.total_tokens ?? 0),
            },
            {
              label: "cached",
              value: formatCount(node.trace.cached_tokens ?? 0),
            },
            {
              label: "http",
              value:
                node.trace.status_code != null
                  ? `HTTP ${node.trace.status_code}`
                  : "-",
            },
            { label: "stop reason", value: node.trace.stop_reason ?? "-" },
          ]}
        />
      </Section>

      {trace ? (
        <Section title="Trace fields">
          <DetailList
            items={[
              { label: "span name", value: node.name },
              { label: "span kind", value: node.spanKind },
              { label: "trace id", value: trace.trace_id },
              { label: "span id", value: trace.span_id },
              { label: "parent span", value: trace.parent_span_id ?? "-" },
              { label: "server.address", value: trace.base_url },
              { label: "http.route", value: trace.endpoint_path },
            ]}
          />
          <RawJson title="request_summary" value={trace.request_summary} />
          <RawJson title="response_summary" value={trace.response_summary} />
          <RawJson title="otel_attributes" value={trace.otel_attributes} />
        </Section>
      ) : null}
    </div>
  )
}

function ToolInspector({
  node,
  trace,
  tab,
}: {
  node: Extract<LoopTimelineNode, { kind: "tool_span" }>
  trace: TraceRecord | null
  tab: InspectorTab
}) {
  const providerRequest = asRecord(trace?.provider_request)
  const toolName = getToolDisplayName(node.trace.model)
  const argumentValue = filterToolArgumentsForInspector(
    providerRequest?.arguments ?? providerRequest ?? {}
  )
  const hasExtraArguments = Object.keys(argumentValue).length > 0
  const outcome = trace?.response_body ?? node.trace.error ?? null

  if (tab === "content") {
    return (
      <div className="space-y-3">
        {hasExtraArguments ? (
          <Section title="Input">
            <StructuredArguments value={argumentValue} />
          </Section>
        ) : null}

        <Section title="Output">
          <ExpandableTextBlock
            value={outcome}
            tone={node.status === "error" ? "danger" : "default"}
          />
        </Section>
      </div>
    )
  }

  if (tab === "events") {
    return (
      <Section title="Events">
        <EventTimeline
          events={
            trace?.events?.length
              ? trace.events.map((event, index) => ({
                  key: `${event.name}-${event.at_ms}-${index}`,
                  name: event.name,
                  at_ms: event.at_ms,
                  summary: summarizeEvent(event),
                  attributes: event.attributes,
                }))
              : buildToolEvents(node)
          }
          emptyLabel="No tool events."
        />
      </Section>
    )
  }

  return (
    <div className="space-y-3">
      <Section title="Attributes">
        <DetailList
          items={[
            { label: "tool", value: toolName },
            { label: "status", value: node.status },
            { label: "operation", value: node.operationName },
            { label: "duration", value: formatTraceDuration(node.durationMs) },
            { label: "started", value: formatDateTime(node.startedAtMs) },
          ]}
        />
      </Section>

      {trace ? (
        <Section title="Trace fields">
          <DetailList
            items={[
              { label: "span name", value: node.name },
              { label: "span kind", value: node.spanKind },
              { label: "trace id", value: trace.trace_id },
              { label: "span id", value: trace.span_id },
              { label: "parent span", value: trace.parent_span_id ?? "-" },
              { label: "provider", value: trace.provider },
              { label: "endpoint", value: trace.endpoint_path },
            ]}
          />
          <RawJson title="request_summary" value={trace.request_summary} />
          <RawJson title="response_summary" value={trace.response_summary} />
          <RawJson title="otel_attributes" value={trace.otel_attributes} />
        </Section>
      ) : null}
    </div>
  )
}

export function TracePanel() {
  const setView = useChatStore((state) => state.setView)
  const turns = useChatStore((state) => state.turns)
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
                <h1 className="text-sm font-semibold tracking-tight">Trace</h1>
                <Badge variant="secondary" className="text-ui-xs">
                  {traceSurface === "overview" ? "overview" : traceView}
                </Badge>
              </div>
              {traceSurface === "overview" ? (
                <div className="text-meta mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-muted-foreground">
                  <span className="tracking-[0.15em] uppercase">Overview</span>
                  <span className="rounded-full border border-border/16 bg-background/60 px-2 py-0.5">
                    {formatOverviewRangeLabel(overviewRange)}
                  </span>
                  <span>
                    {formatCount(
                      overviewDashboard?.overall_summary.unique_models
                    )}{" "}
                    models
                  </span>
                  <span>·</span>
                  <span>
                    {formatCount(
                      overviewDashboard?.overall_summary.total_tool_spans
                    )}{" "}
                    tool spans
                  </span>
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
                <p className="mt-4 text-sm font-medium text-foreground/70">
                  {traceView === "compression"
                    ? "No compression logs yet"
                    : "No traces yet"}
                </p>
                <p className="text-ui mt-1 text-muted-foreground">
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
                            <h3 className="text-heading-sm font-semibold text-foreground">
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
                      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
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
