import { useMemo, useState, type ReactNode } from "react"
import { ExternalLink, Loader2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { asRecord } from "@/lib/trace-inspection"
import type { TraceRecord } from "@/lib/types"
import {
  formatTraceDuration,
  type LoopTimelineNode,
  type TraceLoopGroup,
} from "@/lib/trace-presentation"
import { getToolDisplayName } from "@/lib/tool-display"
import { cn } from "@/lib/utils"

import {
  formatCount,
  formatDateTime,
  loopWindowMs,
} from "./lib/trace-panel-formatters"
import {
  buildRootEvents,
  buildToolEvents,
  summarizeEvent,
} from "./lib/trace-timeline"
import { collectAssistantPreview, collectToolNames } from "./lib/trace-preview"

export type InspectorTab = "content" | "overview" | "events"

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
        "workspace-code text-ui-sm overflow-x-auto rounded-lg border border-border/20 bg-background px-3 py-2.5 whitespace-pre-wrap text-foreground",
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
          type="button"
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

function RetrySummaryList({ trace }: { trace: TraceRecord | null }) {
  const retryEvents = (trace?.events ?? []).filter(
    (event) => event.name === "response.retrying"
  )

  if (retryEvents.length === 0) {
    return <p className="text-caption text-muted-foreground">No retry attempts.</p>
  }

  return (
    <div className="space-y-2">
      {retryEvents.map((event, index) => {
        const attributes = asRecord(event.attributes)
        const attempt = attributes?.attempt
        const maxAttempts = attributes?.max_attempts
        const reason = asString(attributes?.reason)
        const label =
          typeof attempt === "number" && typeof maxAttempts === "number"
            ? `Attempt ${attempt + 1} / ${maxAttempts}`
            : `Retry ${index + 1}`

        return (
          <div
            key={`${event.name}-${event.at_ms}-${index}`}
            className="rounded-lg border border-border/30 bg-background px-3 py-2.5"
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-ui font-medium text-foreground">{label}</span>
              <span className="text-meta text-muted-foreground">
                {formatDateTime(event.at_ms)}
              </span>
            </div>
            <p className="text-caption mt-1 text-muted-foreground">
              {reason ?? "Retry triggered"}
            </p>
          </div>
        )
      })}
    </div>
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

export function LoopInspector({
  group,
  tab,
}: {
  group: TraceLoopGroup
  tab: InspectorTab
}) {
  const rootNode = group.timeline[0]
  const systemPromptPreview =
    rootNode?.kind === "agent_root" ? rootNode.systemPromptPreview : null

  if (tab === "content") {
    return (
      <div className="space-y-3">
        <Section title="Input">
          <FieldBlock label="System prompt">
            <TextBlock
              value={systemPromptPreview}
              className="trace-accent-surface"
            />
          </FieldBlock>
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

export function LlmInspector({
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
              className="text-ui-sm h-7 px-2.5"
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
            {
              label: "retries",
              value: String(
                (trace?.events ?? []).filter(
                  (event) => event.name === "response.retrying"
                ).length
              ),
            },
          ]}
        />
      </Section>

      {trace ? (
        <Section title="Retry attempts">
          <RetrySummaryList trace={trace} />
        </Section>
      ) : null}

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

export function ToolInspector({
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
