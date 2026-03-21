import { useEffect, useMemo, useState, type ReactNode } from "react"
import {
  ArrowLeft,
  Bot,
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
import type { TraceEvent, TraceRecord } from "@/lib/types"
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

function nodeOffsetPercent(node: LoopTimelineNode, group: TraceLoopGroup) {
  const window = loopWindowMs(group)
  return Math.max(0, ((node.startedAtMs - group.startedAtMs) / window) * 100)
}

function nodeWidthPercent(node: LoopTimelineNode, group: TraceLoopGroup) {
  const window = loopWindowMs(group)
  const raw = ((Math.max(node.durationMs, 1) || 1) / window) * 100
  return Math.max(2, Math.min(100, raw))
}

function relativeOffsetLabel(node: LoopTimelineNode, group: TraceLoopGroup) {
  const delta = Math.max(0, node.startedAtMs - group.startedAtMs)
  return `+${formatTraceDuration(delta)}`
}

function nodeTitle(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return "invoke_agent"
    case "llm_span":
      return node.trace.model
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
        ? `${node.operationName} · ${formatCount(node.trace.total_tokens)} tok`
        : node.operationName
    case "tool_span":
      return node.trace.endpoint_path.startsWith("/tools/")
        ? node.operationName
        : `${node.operationName} · ${node.trace.endpoint_path}`
  }
}

function nodeTone(node: LoopTimelineNode) {
  if (node.kind === "agent_root") {
    return {
      frame: "border-sky-500/20 bg-sky-500/[0.04]",
      dot: "border-sky-500/35 bg-sky-500/20 text-sky-100",
      bar: "border-sky-500/35 bg-sky-500/18",
    }
  }

  if (node.kind === "tool_span") {
    return node.status === "error"
      ? {
          frame: "border-destructive/25 bg-destructive/[0.04]",
          dot: "border-destructive/30 bg-destructive/15 text-destructive",
          bar: "border-destructive/30 bg-destructive/20",
        }
      : {
          frame: "border-amber-500/20 bg-amber-500/[0.04]",
          dot: "border-amber-500/30 bg-amber-500/15 text-amber-100",
          bar: "border-amber-500/30 bg-amber-500/20",
        }
  }

  return node.status === "error"
    ? {
        frame: "border-destructive/25 bg-destructive/[0.04]",
        dot: "border-destructive/30 bg-destructive/15 text-destructive",
        bar: "border-destructive/30 bg-destructive/20",
      }
    : {
        frame: "border-border/40 bg-background/80",
        dot: "border-border/45 bg-muted/35 text-foreground/80",
        bar: "border-border/40 bg-foreground/[0.08]",
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
    <dl className="divide-y divide-border/25 overflow-hidden rounded-lg border border-border/35 bg-background/70">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex flex-wrap items-start justify-between gap-x-2 gap-y-0.5 px-2 py-1.5"
        >
          <dt className="text-[10px] font-medium tracking-[0.12em] text-muted-foreground uppercase">
            {item.label}
          </dt>
          <dd className="min-w-0 text-right text-[11px] leading-4 text-foreground">
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
    <section className="space-y-2 rounded-xl border border-border/35 bg-background/70 p-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
          {title}
        </h3>
        {action}
      </div>
      {children}
    </section>
  )
}

function TabButton({
  active,
  children,
  onClick,
}: {
  active: boolean
  children: ReactNode
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "rounded-md px-2 py-0.5 text-[10px] font-medium tracking-[0.08em] uppercase transition-all active:scale-[0.96]",
        active
          ? "bg-foreground text-background"
          : "text-muted-foreground hover:bg-accent/40 hover:text-foreground"
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
      <p className="text-[10px] font-medium tracking-[0.12em] text-muted-foreground uppercase">
        {label}
      </p>
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
    return <p className="text-[12px] text-muted-foreground">-</p>
  }

  return (
    <pre
      className={cn(
        "overflow-x-auto rounded-lg border border-border/25 bg-muted/20 px-2 py-1.5 text-[11px] leading-4 whitespace-pre-wrap text-foreground",
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
    return <p className="text-[12px] text-muted-foreground">No arguments.</p>
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
    return <p className="text-[12px] text-muted-foreground">-</p>
  }

  const needsCollapse = value.length > 320 || value.split("\n").length > 10

  return (
    <div className="space-y-2">
      <pre
        className={cn(
          "overflow-auto rounded-lg border border-border/25 bg-muted/20 px-2.5 py-2 text-[12px] leading-5 whitespace-pre-wrap text-foreground",
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
          className="text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground"
        >
          {open ? "Collapse" : "Expand"}
        </button>
      ) : null}
    </div>
  )
}

function RawJson({ title, value }: { title: string; value: unknown }) {
  return (
    <Collapsible className="rounded-lg border border-border/35 bg-muted/[0.02]">
      <CollapsibleTrigger className="flex w-full items-center justify-between px-2.5 py-2 text-left">
        <span className="text-[12px] font-medium text-foreground">{title}</span>
        <span className="text-[11px] text-muted-foreground">JSON</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-border/25 px-2.5 py-2">
        <pre className="overflow-x-auto rounded-md bg-muted/25 p-3 text-[11px] leading-5 text-foreground">
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
    return <p className="text-[12px] text-muted-foreground">{emptyLabel}</p>
  }

  return (
    <div className="relative space-y-1 pl-4 before:absolute before:top-2 before:bottom-2 before:left-[7px] before:w-px before:bg-border/25">
      {events.map((event) => (
        <Collapsible
          key={event.key}
          className="relative rounded-lg border border-border/35 bg-background/80 before:absolute before:top-3 before:-left-[12px] before:size-1.5 before:rounded-full before:bg-foreground/35"
        >
          <CollapsibleTrigger className="flex w-full items-start justify-between gap-3 px-2 py-1.5 text-left">
            <div className="min-w-0 space-y-0.5">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[11px] font-medium text-foreground">
                  {event.name}
                </span>
                <span className="text-[10px] text-muted-foreground">
                  {formatDateTime(event.at_ms)}
                </span>
              </div>
              {event.summary ? (
                <p className="text-[11px] leading-4 text-muted-foreground">
                  {event.summary}
                </p>
              ) : null}
            </div>
          </CollapsibleTrigger>
          {event.attributes && Object.keys(event.attributes).length > 0 ? (
            <CollapsibleContent className="border-t border-border/25 px-2 py-1.5">
              <pre className="overflow-x-auto rounded-md bg-muted/25 p-2 text-[11px] leading-5 text-foreground">
                {JSON.stringify(event.attributes, null, 2)}
              </pre>
            </CollapsibleContent>
          ) : null}
        </Collapsible>
      ))}
    </div>
  )
}

function TraceActiveStrip({ group }: { group: TraceLoopGroup }) {
  return (
    <div className="overflow-hidden rounded-xl border border-t-2 border-border/40 border-t-foreground/8 bg-card">
      <div className="px-2.5 py-2.5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="max-w-[760px] truncate text-[16px] font-semibold tracking-tight text-foreground">
                {formatTraceLoopHeadline(group, {
                  compressionLabel: "Context compression log",
                  maxLength: 180,
                })}
              </span>
              <Badge
                variant={loopBadgeVariant(group.finalStatus)}
                className="text-[10px]"
              >
                {group.finalStatus}
              </Badge>
              <span className="rounded-md border border-border/35 bg-background/40 px-2 py-1 font-mono text-[11px] text-muted-foreground">
                {compactId(group.key, 12, 8)}
              </span>
            </div>
            <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
              <span>turn {group.turnId}</span>
              <span className="text-border">/</span>
              <span>run {compactId(group.runId, 8, 6)}</span>
              <span className="text-border">/</span>
              <span>{formatDateTime(group.startedAtMs)}</span>
              <span className="text-border">/</span>
              <span>{formatTraceDuration(loopWindowMs(group))}</span>
              <span className="text-border">/</span>
              <span>{group.stepCount} llm</span>
              <span className="text-border">/</span>
              <span>{group.toolCount} tool</span>
              {group.failedToolCount > 0 ? (
                <>
                  <span className="text-border">/</span>
                  <span className="text-amber-700 dark:text-amber-300">
                    {group.failedToolCount} issue
                  </span>
                </>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function WaterfallScale({ group }: { group: TraceLoopGroup }) {
  const window = loopWindowMs(group)
  const markers = [0, 25, 50, 75, 100]
  const markerLabels = markers.map((marker) =>
    formatTraceDuration(Math.round((window * marker) / 100))
  )

  return (
    <div className="grid grid-cols-[minmax(180px,240px)_minmax(0,1fr)_72px] items-end gap-2 px-0.5 pb-1.5">
      <div className="text-[10px] tracking-[0.12em] text-muted-foreground uppercase">
        span
      </div>
      <div className="space-y-1">
        <div className="flex items-center justify-between gap-2 px-0.5 font-mono text-[10px] text-muted-foreground/90">
          {markerLabels.map((label, index) => (
            <span
              key={`${label}-${index}`}
              className={cn(
                "shrink-0",
                index === 0 && "text-left",
                index > 0 && index < markerLabels.length - 1 && "text-center",
                index === markerLabels.length - 1 && "text-right"
              )}
            >
              {label}
            </span>
          ))}
        </div>
        <div className="relative h-3 overflow-hidden rounded-md border border-border/20 bg-background/40">
          <div className="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-border/30" />
          {markers.map((marker) => (
            <div
              key={marker}
              className="absolute top-0 bottom-0"
              style={{ left: `${marker}%` }}
            >
              <div className="absolute inset-y-0 w-px bg-border/25" />
            </div>
          ))}
        </div>
      </div>
      <div className="text-right text-[10px] tracking-[0.12em] text-muted-foreground uppercase">
        dur
      </div>
    </div>
  )
}

function WaterfallRow({
  group,
  node,
  selected,
  loading,
  onSelect,
}: {
  group: TraceLoopGroup
  node: LoopTimelineNode
  selected: boolean
  loading: boolean
  onSelect: () => void
}) {
  const tone = nodeTone(node)
  const offset = nodeOffsetPercent(node, group)
  const width = nodeWidthPercent(node, group)

  return (
    <button
      onClick={onSelect}
      className={cn(
        "w-full rounded-lg border px-2 py-1.5 text-left transition-all active:scale-[0.99]",
        selected ? "border-foreground/20 bg-accent/35" : tone.frame
      )}
    >
      <div className="grid grid-cols-[minmax(180px,240px)_minmax(0,1fr)_72px] items-center gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span
              className={cn(
                "flex size-4 shrink-0 items-center justify-center rounded-full border",
                tone.dot
              )}
            >
              {node.kind === "agent_root" ? (
                <Waypoints className="size-2.5" />
              ) : node.kind === "llm_span" ? (
                <Bot className="size-2.5" />
              ) : (
                <Wrench className="size-2.5" />
              )}
            </span>
            <span className="truncate text-[11px] font-medium text-foreground">
              {nodeTitle(node)}
            </span>
            {node.kind !== "agent_root" &&
            "status" in node &&
            node.status === "error" ? (
              <span className="rounded-sm border border-destructive/30 bg-destructive/[0.08] px-1.5 py-0.5 text-[10px] font-medium text-destructive uppercase">
                err
              </span>
            ) : null}
            {loading ? (
              <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
            ) : null}
          </div>
          <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 pl-7 text-[10px] text-muted-foreground">
            <span>{nodeSubtitle(node)}</span>
            <span>{relativeOffsetLabel(node, group)}</span>
          </div>
        </div>

        <div className="relative h-7 overflow-hidden rounded-md border border-border/20 bg-background/35">
          <div className="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-border/30" />
          <div
            className={cn(
              "absolute top-[8px] h-[14px] rounded-sm border",
              tone.bar
            )}
            style={{
              left: `${offset}%`,
              width: `${Math.min(100 - offset, width)}%`,
            }}
          >
            <div className="absolute top-1/2 left-1 size-1 -translate-y-1/2 rounded-full bg-foreground/55" />
          </div>
        </div>

        <div className="text-right">
          <div className="font-mono text-[10px] text-foreground">
            {formatTraceDuration(node.durationMs)}
          </div>
          <div className="mt-0.5 text-[10px] text-muted-foreground">
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
      <Section title="Conversation">
        <div className="space-y-3">
          <FieldBlock label="User message">
            <TextBlock value={group.userMessage} className="bg-background/80" />
          </FieldBlock>
          <FieldBlock label="Assistant reply">
            <TextBlock
              value={group.assistantMessage}
              className="border-sky-500/20 bg-sky-500/[0.05]"
            />
          </FieldBlock>
        </div>
      </Section>
    )
  }

  if (tab === "events") {
    return (
      <Section title="Root events">
        <EventTimeline
          events={buildRootEvents(group)}
          emptyLabel="No root events."
        />
      </Section>
    )
  }

  return (
    <div className="space-y-3">
      <Section title="Root span">
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

      <Collapsible className="rounded-xl border border-border/35 bg-background/70">
        <CollapsibleTrigger className="flex w-full items-center justify-between px-3 py-2.5 text-left">
          <span className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
            Trace fields
          </span>
          <span className="text-[11px] text-muted-foreground">IDs</span>
        </CollapsibleTrigger>
        <CollapsibleContent className="border-t border-border/25 px-3 py-3">
          <DetailList
            items={[
              { label: "trace id", value: group.key },
              { label: "run id", value: group.runId },
              { label: "root span", value: group.timeline[0]?.id ?? "-" },
            ]}
          />
        </CollapsibleContent>
      </Collapsible>
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
  const systemPrompts = collectSystemPrompts(trace)
  const toolNames = collectToolNames(trace)
  const assistantPreview = collectAssistantPreview(trace)

  if (tab === "content") {
    return (
      <div className="space-y-3">
        <Section
          title="Prompt frame"
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
                <p className="text-[12px] text-muted-foreground">-</p>
              ) : (
                <div className="space-y-2">
                  {systemPrompts.map((prompt, index) => (
                    <TextBlock
                      key={`${index}-${prompt.slice(0, 24)}`}
                      value={prompt}
                      className="border-sky-500/15 bg-sky-500/[0.04]"
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
                <p className="text-[12px] text-muted-foreground">-</p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {toolNames.map((tool) => (
                    <Badge
                      key={tool}
                      variant="secondary"
                      className="text-[10px]"
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
      <Section title="Span events">
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
      <Section title="Span summary">
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
        <Collapsible className="rounded-xl border border-border/35 bg-background/70">
          <CollapsibleTrigger className="flex w-full items-center justify-between px-3 py-2.5 text-left">
            <span className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
              Trace fields
            </span>
            <span className="text-[11px] text-muted-foreground">Raw</span>
          </CollapsibleTrigger>
          <CollapsibleContent className="space-y-3 border-t border-border/25 px-3 py-3">
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
          </CollapsibleContent>
        </Collapsible>
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
          <Section title="Arguments">
            <StructuredArguments value={argumentValue} />
          </Section>
        ) : null}

        <Section title="Outcome">
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
      <Section title="Span events">
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
      <Section title="Span summary">
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
        <Collapsible className="rounded-xl border border-border/35 bg-background/70">
          <CollapsibleTrigger className="flex w-full items-center justify-between px-3 py-2.5 text-left">
            <span className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
              Trace fields
            </span>
            <span className="text-[11px] text-muted-foreground">Raw</span>
          </CollapsibleTrigger>
          <CollapsibleContent className="space-y-3 border-t border-border/25 px-3 py-3">
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
          </CollapsibleContent>
        </Collapsible>
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
  const refreshOverview = useTraceOverviewStore((state) => state.refresh)

  const [payloadOpen, setPayloadOpen] = useState(false)
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("content")

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

  const inspectedTrace =
    activeNode?.kind === "agent_root"
      ? null
      : (selectedLoop?.trace_details.find(
          (trace) => trace.id === activeNode?.trace.id
        ) ??
        (selectedTrace?.id === activeNode?.trace.id ? selectedTrace : null))

  const traceDescription =
    traceSurface === "overview"
      ? "Inspect request volume, token usage, failures, and activity before drilling into individual spans."
      : traceView === "compression"
        ? "Review compression runs, loop timing, and payload details without mixing them into the main conversation trace stream."
        : "Inspect conversation loops, waterfall timing, and span payloads from the current workspace."

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-center justify-between gap-2 border-b border-border/30 px-4 py-2.5">
        <div className="flex items-start gap-3">
          <button
            onClick={() => setView("chat")}
            className="mt-0.5 flex size-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <ArrowLeft className="size-3.5" />
          </button>
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="text-sm font-semibold tracking-tight">Trace</h1>
              <Badge variant="secondary" className="text-[10px]">
                {traceSurface === "overview" ? "overview" : traceView}
              </Badge>
            </div>
            <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
              {traceDescription}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              traceSurface === "overview"
                ? refreshOverview().catch(() => {})
                : refreshTraces({ page: tracePage, view: traceView })
            }
          >
            <RefreshCw className="size-3.5" />
            Refresh
          </Button>
        </div>
      </div>

      {traceSurface === "overview" ? (
        <TraceOverviewPanel />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-3">
          <div className="mx-auto flex min-h-0 w-full max-w-[1440px] flex-1 flex-col gap-2">
            {traceError ? (
              <div className="shrink-0 rounded-xl border border-destructive/25 bg-destructive/[0.05] px-4 py-3 text-[12px] text-destructive">
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
                <p className="mt-1 text-[13px] text-muted-foreground">
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
              <div className="grid min-h-0 flex-1 overflow-hidden rounded-xl border border-border/30 bg-card/70 shadow-[var(--workspace-shadow)] xl:grid-cols-[minmax(0,1.18fr)_360px]">
                <div className="flex min-h-0 flex-col overflow-hidden">
                  <div className="shrink-0 border-b border-border/25 px-2.5 py-2">
                    <div className="flex items-center justify-between gap-2">
                      <p className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
                        Waterfall
                      </p>
                      {activeGroup ? (
                        <span className="font-mono text-[11px] text-muted-foreground">
                          {formatTraceDuration(loopWindowMs(activeGroup))}
                        </span>
                      ) : null}
                    </div>
                  </div>

                  <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
                    {activeGroup ? (
                      <div className="space-y-1.5">
                        <WaterfallScale group={activeGroup} />
                        <div className="space-y-1">
                          {activeGroup.timeline.map((node) => (
                            <WaterfallRow
                              key={node.id}
                              group={activeGroup}
                              node={node}
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
                      </div>
                    ) : null}
                  </div>
                </div>

                <div className="flex min-h-0 flex-col overflow-hidden border-l border-border/25">
                  <div className="shrink-0 border-b border-border/25 px-2.5 py-2">
                    <div className="flex items-center justify-between gap-2">
                      <div>
                        <p className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
                          Inspector
                        </p>
                        {activeNode ? (
                          <p className="mt-0.5 text-[11px] text-muted-foreground">
                            {nodeTitle(activeNode)} · {activeNode.kind}
                          </p>
                        ) : null}
                      </div>
                      <div className="rounded-lg border border-border/35 bg-muted/20 p-0.5">
                        <div className="flex items-center gap-1">
                          <TabButton
                            active={inspectorTab === "content"}
                            onClick={() => setInspectorTab("content")}
                          >
                            content
                          </TabButton>
                          <TabButton
                            active={inspectorTab === "overview"}
                            onClick={() => setInspectorTab("overview")}
                          >
                            overview
                          </TabButton>
                          <TabButton
                            active={inspectorTab === "events"}
                            onClick={() => setInspectorTab("events")}
                          >
                            events
                          </TabButton>
                        </div>
                      </div>
                    </div>
                  </div>

                  <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
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
