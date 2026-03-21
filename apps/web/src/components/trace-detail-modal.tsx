import { AlertTriangle, Check, CheckCircle2, Copy, Loader2 } from "lucide-react"
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"

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
import {
  Dialog,
  DialogBackdrop,
  DialogBody,
  DialogClose,
  DialogDescription,
  DialogHeader,
  DialogPopup,
  DialogPortal,
  DialogTitle,
} from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
  asArray,
  asRecord,
  asString,
  extractTraceText,
  type JsonRecord,
} from "@/lib/trace-inspection"
import { getToolDisplayName } from "@/lib/tool-display"
import type { TraceRecord } from "@/lib/types"
import { cn } from "@/lib/utils"

type PromptEntry = {
  label: string
  source: string
  content: string
}

const COPY_RESET_DELAY_MS = 1200

function asNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null
}

function asBoolean(value: unknown): boolean | null {
  return typeof value === "boolean" ? value : null
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

function formatJson(value: unknown) {
  return value == null ? "-" : JSON.stringify(value, null, 2)
}

function formatPrimitive(value: unknown) {
  if (value == null) return "-"
  if (typeof value === "boolean") return value ? "Yes" : "No"
  if (Array.isArray(value))
    return value.length === 0 ? "-" : `${value.length} items`
  if (typeof value === "object") return JSON.stringify(value)
  return String(value)
}

async function copyText(value: string): Promise<boolean> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value)
      return true
    } catch {
      return false
    }
  }

  return false
}

function payloadCopyText(value: unknown) {
  if (typeof value === "string") {
    return value
  }
  return formatJson(value)
}

function truncate(text: string, max: number) {
  return text.length > max ? `${text.slice(0, max)}…` : text
}

function summarizeSchema(schema: JsonRecord | null) {
  const properties = asRecord(schema?.properties)
  const propertyCount = properties ? Object.keys(properties).length : 0
  const requiredCount = asArray(schema?.required).filter(
    (value): value is string => typeof value === "string"
  ).length

  return { propertyCount, requiredCount }
}

function normalizeToolDefinition(tool: unknown, index: number) {
  const record = asRecord(tool)
  const fnRecord = asRecord(record?.function)
  const name =
    asString(record?.name) ?? asString(fnRecord?.name) ?? `tool_${index}`
  const description =
    asString(record?.description) ?? asString(fnRecord?.description) ?? null
  const parameters =
    asRecord(record?.parameters) ?? asRecord(fnRecord?.parameters)

  return { name, description, parameters }
}

function normalizeMessageKind(item: JsonRecord) {
  return asString(item.role) ?? asString(item.type) ?? "unknown"
}

function renderToolBadges(toolNames: string[]) {
  if (toolNames.length === 0) {
    return <span className="text-muted-foreground">None</span>
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      {toolNames.map((tool) => (
        <Badge key={tool} variant="secondary" className="text-[11px]">
          {tool}
        </Badge>
      ))}
    </div>
  )
}

function roleBadgeVariant(role: string) {
  switch (role) {
    case "system":
      return "outline" as const
    case "user":
      return "default" as const
    case "assistant":
      return "secondary" as const
    case "tool":
    case "function_call":
    case "function_call_output":
      return "secondary" as const
    default:
      return "outline" as const
  }
}

function messageToneClasses(kind: string) {
  void kind
  return "border-border/45 bg-background/80"
}

function extractContent(item: JsonRecord): string {
  const kind = asString(item.type)
  if (kind === "function_call") {
    const name = asString(item.name) ?? "function_call"
    const argumentsText = asString(item.arguments)
    return argumentsText ? `${name}(${truncate(argumentsText, 120)})` : name
  }

  if (kind === "function_call_output") {
    return asString(item.output) ?? "[function call output]"
  }

  if (Array.isArray(item.tool_calls)) {
    const names = item.tool_calls
      .map((toolCall) => {
        const functionRecord = asRecord(asRecord(toolCall)?.function)
        return asString(functionRecord?.name)
      })
      .filter((value): value is string => Boolean(value))

    return names.length > 0
      ? `[tool calls: ${names.join(", ")}]`
      : "[tool calls]"
  }

  return extractTraceText(item.content) || extractTraceText(item.output) || ""
}

function summarizeMessageKinds(items: unknown[]) {
  const counts = new Map<string, number>()

  for (const item of items) {
    const record = asRecord(item)
    const kind = record ? normalizeMessageKind(record) : "unknown"
    counts.set(kind, (counts.get(kind) ?? 0) + 1)
  }

  return Array.from(counts.entries())
}

function DetailList({
  items,
}: {
  items: Array<{ label: string; value: ReactNode }>
}) {
  return (
    <dl className="divide-y divide-border/30 rounded-lg border border-border/40 bg-background/70">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 px-3 py-2"
        >
          <dt className="shrink-0 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {item.label}
          </dt>
          <dd className="min-w-0 text-[13px] text-foreground sm:text-right [&>div]:justify-end">
            {item.value}
          </dd>
        </div>
      ))}
    </dl>
  )
}

function CompactDetailList({
  items,
}: {
  items: Array<{ label: string; value: ReactNode }>
}) {
  return (
    <dl className="space-y-1.5">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex flex-wrap items-baseline gap-x-2 gap-y-1"
        >
          <dt className="shrink-0 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {item.label}
          </dt>
          <dd className="min-w-0 text-[12px] break-all text-foreground/90">
            {item.value}
          </dd>
        </div>
      ))}
    </dl>
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
    return <p className="text-[13px] text-muted-foreground">-</p>
  }

  return (
    <pre
      className={cn(
        "overflow-x-auto rounded-lg bg-muted/40 p-3 text-[12px] leading-6 whitespace-pre-wrap text-foreground",
        className
      )}
    >
      {value}
    </pre>
  )
}

function RawJsonSection({ title, value }: { title: string; value: unknown }) {
  return (
    <Collapsible className="rounded-lg border border-border/50 bg-muted/15 px-3 py-2">
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-3 text-left text-sm font-medium text-foreground">
        <span>{title}</span>
        <span className="text-xs text-muted-foreground">Raw JSON</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-3">
        <pre className="overflow-x-auto rounded-lg bg-muted/40 p-3 text-[12px] leading-6 text-foreground">
          {formatJson(value)}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  )
}

function SectionPanel({
  title,
  description,
  meta,
  children,
  className,
}: {
  title: string
  description?: string
  meta?: ReactNode
  children: ReactNode
  className?: string
}) {
  return (
    <section
      className={cn(
        "rounded-xl border border-border/50 bg-muted/[0.04]",
        className
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border/40 px-4 py-3">
        <div className="space-y-1">
          <h3 className="text-sm font-semibold text-foreground">{title}</h3>
          {description ? (
            <p className="text-[12px] text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {meta ? <div className="flex flex-wrap gap-1.5">{meta}</div> : null}
      </div>
      <div className="px-4 py-4">{children}</div>
    </section>
  )
}

function ParameterSchemaTable({ schema }: { schema: JsonRecord }) {
  const properties = asRecord(schema.properties)
  if (!properties) {
    return (
      <pre className="overflow-x-auto rounded-lg bg-muted/40 p-3 text-[12px] leading-6 text-foreground">
        {formatJson(schema)}
      </pre>
    )
  }

  const requiredSet = new Set(
    asArray(schema.required).filter(
      (value): value is string => typeof value === "string"
    )
  )
  const entries = Object.entries(properties)

  if (entries.length === 0) {
    return (
      <p className="text-[12px] text-muted-foreground">
        No properties defined.
      </p>
    )
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[12px]">
        <thead>
          <tr className="border-b border-border/40 text-left text-[11px] text-muted-foreground uppercase">
            <th className="pr-3 pb-2 font-medium">Name</th>
            <th className="pr-3 pb-2 font-medium">Type</th>
            <th className="pr-3 pb-2 font-medium">Required</th>
            <th className="pb-2 font-medium">Description</th>
          </tr>
        </thead>
        <tbody>
          {entries.map(([name, value]) => {
            const record = asRecord(value)
            const type = asString(record?.type) ?? "any"
            const description = asString(record?.description) ?? ""
            const isRequired = requiredSet.has(name)

            return (
              <tr key={name} className="border-b border-border/20 align-top">
                <td className="py-2 pr-3 font-mono text-foreground">{name}</td>
                <td className="py-2 pr-3">
                  <Badge variant="outline" className="text-[10px]">
                    {type}
                  </Badge>
                </td>
                <td className="py-2 pr-3">
                  {isRequired ? (
                    <Badge variant="destructive" className="text-[10px]">
                      required
                    </Badge>
                  ) : (
                    <span className="text-muted-foreground">optional</span>
                  )}
                </td>
                <td className="py-2 text-muted-foreground">
                  {description || "—"}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

function SystemPromptSection({ prompts }: { prompts: PromptEntry[] }) {
  const totalCharacters = prompts.reduce(
    (sum, prompt) => sum + prompt.content.length,
    0
  )

  return (
    <SectionPanel
      title="System prompts"
      description="Instruction text and system-role messages"
      className="trace-accent-surface"
      meta={
        <>
          <Badge variant="outline" className="text-[11px]">
            {prompts.length}
          </Badge>
          <span className="text-[11px] text-muted-foreground">
            {totalCharacters} chars
          </span>
        </>
      }
    >
      {prompts.length === 0 ? (
        <p className="text-[13px] text-muted-foreground">
          No system prompt was captured in the provider request.
        </p>
      ) : (
        <div className="space-y-3">
          {prompts.map((prompt, index) => (
            <div
              key={`${prompt.source}-${index}`}
              className="trace-accent-surface rounded-xl p-3"
            >
              <div className="mb-2 flex flex-wrap items-center gap-2">
                <Badge variant="outline" className="text-[11px]">
                  {prompt.label}
                </Badge>
                <span className="text-[11px] text-muted-foreground">
                  {prompt.source}
                </span>
              </div>
              <TextBlock
                value={prompt.content}
                className="trace-accent-fill max-h-[320px]"
              />
            </div>
          ))}
        </div>
      )}
    </SectionPanel>
  )
}

function ToolListSection({ tools }: { tools: unknown[] }) {
  const normalized = tools.map((tool, index) =>
    normalizeToolDefinition(tool, index)
  )

  return (
    <SectionPanel
      title="Tool definitions"
      description="Schemas sent upstream."
      meta={
        <>
          <Badge variant="outline" className="text-[11px]">
            {normalized.length} tool{normalized.length === 1 ? "" : "s"}
          </Badge>
        </>
      }
    >
      {normalized.length === 0 ? (
        <p className="text-[13px] text-muted-foreground">No tools defined.</p>
      ) : (
        <div className="space-y-2">
          {normalized.map((tool, index) => {
            const { propertyCount, requiredCount } = summarizeSchema(
              tool.parameters
            )

            return (
              <Collapsible
                key={`${tool.name}-${index}`}
                defaultOpen={index === 0}
                className="rounded-xl border border-border/50 bg-background/70"
              >
                <CollapsibleTrigger className="flex w-full flex-wrap items-start justify-between gap-3 px-4 py-3 text-left">
                  <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="secondary" className="text-[11px]">
                        {tool.name}
                      </Badge>
                      <Badge variant="outline" className="text-[10px]">
                        {propertyCount} field{propertyCount === 1 ? "" : "s"}
                      </Badge>
                      <Badge variant="outline" className="text-[10px]">
                        {requiredCount} required
                      </Badge>
                    </div>
                    <p className="text-[12px] text-muted-foreground">
                      {tool.description ?? "No tool description provided."}
                    </p>
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    schema
                  </span>
                </CollapsibleTrigger>
                <CollapsibleContent className="border-t border-border/30 px-4 py-3">
                  {tool.parameters ? (
                    <div className="space-y-3">
                      <ParameterSchemaTable schema={tool.parameters} />
                      <RawJsonSection
                        title={`${tool.name} schema`}
                        value={tool.parameters}
                      />
                    </div>
                  ) : (
                    <p className="text-[12px] text-muted-foreground">
                      No parameter schema.
                    </p>
                  )}
                </CollapsibleContent>
              </Collapsible>
            )
          })}
        </div>
      )}
    </SectionPanel>
  )
}

function MessageListSection({
  items,
  title,
  description,
}: {
  items: unknown[]
  title: string
  description: string
}) {
  const counts = summarizeMessageKinds(items)
  const countSummary = counts
    .map(([kind, count]) => `${kind} ${count}`)
    .join(" · ")

  return (
    <SectionPanel
      title={title}
      description={description}
      meta={
        <span className="text-[11px] text-muted-foreground">
          {countSummary || "No items"}
        </span>
      }
    >
      <MessageTimelineList items={items} />
    </SectionPanel>
  )
}

function MessageTimelineList({
  items,
  compact = false,
  defaultOpenSystem = true,
}: {
  items: unknown[]
  compact?: boolean
  defaultOpenSystem?: boolean
}) {
  const counts = summarizeMessageKinds(items)
  const countSummary = counts.map(([kind, count]) => `${kind} ${count}`)

  if (items.length === 0) {
    return <p className="text-[13px] text-muted-foreground">No items.</p>
  }

  return (
    <div className="space-y-2">
      {compact && countSummary.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
          {countSummary.map((item) => (
            <Badge key={item} variant="outline" className="text-[10px]">
              {item}
            </Badge>
          ))}
        </div>
      ) : null}

      <div
        className={cn(
          compact &&
            "relative space-y-2 pl-4 before:absolute before:top-2 before:bottom-2 before:left-[7px] before:w-px before:bg-border/30",
          !compact && "space-y-2"
        )}
      >
        {items.map((item, index) => {
          const record = asRecord(item)
          const kind = record ? normalizeMessageKind(record) : "unknown"
          const preview = record
            ? extractContent(record)
            : typeof item === "string"
              ? item
              : ""
          const toolName =
            asString(record?.name) ?? asString(asRecord(record?.function)?.name)
          const displayToolName = toolName ? getToolDisplayName(toolName) : null
          const toolCalls = asArray(record?.tool_calls)
          const callId =
            asString(record?.call_id) ?? asString(record?.tool_call_id)
          const detailText = [
            displayToolName ? `tool ${displayToolName}` : null,
            !displayToolName && callId ? `call ${callId}` : null,
            toolCalls.length > 0
              ? `${toolCalls.length} tool call${toolCalls.length === 1 ? "" : "s"}`
              : null,
          ]
            .filter((part): part is string => Boolean(part))
            .join(" · ")

          return (
            <Collapsible
              key={`${kind}-${index}`}
              defaultOpen={defaultOpenSystem && kind === "system" && !compact}
              className={cn(
                "rounded-xl border bg-background/80",
                compact &&
                  "relative before:absolute before:top-3.5 before:-left-[12px] before:size-1.5 before:rounded-full before:bg-foreground/35",
                messageToneClasses(kind)
              )}
            >
              <CollapsibleTrigger className="flex w-full flex-wrap items-start justify-between gap-3 px-3 py-2.5 text-left">
                <div className="min-w-0 space-y-1.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[10px] text-muted-foreground">
                      #{index + 1}
                    </span>
                    <Badge
                      variant={roleBadgeVariant(kind)}
                      className="h-4 px-1.5 text-[10px]"
                    >
                      {kind}
                    </Badge>
                  </div>
                  <p className="text-[12px] leading-5 text-foreground/85">
                    {preview
                      ? truncate(preview, compact ? 260 : 220)
                      : "No preview available."}
                  </p>
                  {detailText ? (
                    <p className="text-[11px] text-muted-foreground/80">
                      {detailText}
                    </p>
                  ) : null}
                </div>
                <span className="text-[10px] text-muted-foreground">raw</span>
              </CollapsibleTrigger>
              <CollapsibleContent className="border-t border-border/30 px-3 py-3">
                <pre
                  className={cn(
                    "overflow-auto rounded-lg bg-muted/35 p-3 text-[12px] leading-6 whitespace-pre-wrap text-foreground",
                    compact ? "max-h-[420px]" : "max-h-[360px]"
                  )}
                >
                  {formatJson(record ?? item)}
                </pre>
              </CollapsibleContent>
            </Collapsible>
          )
        })}
      </div>
    </div>
  )
}

function summarizeChatMessages(messages: unknown[]) {
  let system = 0
  let user = 0
  let assistant = 0
  let tool = 0
  let assistantToolCalls = 0

  for (const item of messages) {
    const record = asRecord(item)
    const role = asString(record?.role)
    if (role === "system") system += 1
    if (role === "user") user += 1
    if (role === "assistant") assistant += 1
    if (role === "tool") tool += 1
    if (Array.isArray(record?.tool_calls) && record.tool_calls.length > 0) {
      assistantToolCalls += 1
    }
  }

  return { system, user, assistant, tool, assistantToolCalls }
}

function ResponsesRequestContextCard({
  request,
  summary,
}: {
  request: JsonRecord | null
  summary: JsonRecord | null
}) {
  const input = asArray(request?.input)
  const toolNames = asArray(summary?.tool_names).filter(
    (value): value is string => typeof value === "string"
  )

  const prompts: PromptEntry[] = []
  const instructions = asString(request?.instructions)
  if (instructions) {
    prompts.push({
      label: "instructions",
      source: "request.instructions",
      content: instructions,
    })
  }

  let systemPromptIndex = 0
  input.forEach((item, index) => {
    const record = asRecord(item)
    if (!record || asString(record.role) !== "system") return
    const content = extractContent(record)
    if (!content) return
    systemPromptIndex += 1
    prompts.push({
      label: `system #${systemPromptIndex}`,
      source: `input[${index}]`,
      content,
    })
  })

  return (
    <Collapsible
      defaultOpen={false}
      className="rounded-xl border border-border/50 bg-muted/[0.04]"
    >
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold text-foreground">
              Provider request
            </h3>
            <Badge variant="outline" className="text-[10px]">
              responses
            </Badge>
          </div>
          <p className="text-[12px] text-muted-foreground">
            {input.length} items · {prompts.length} system
          </p>
        </div>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-border/40 px-4 py-4">
        <div className="space-y-4">
          <DetailList
            items={[
              {
                label: "Model",
                value: formatPrimitive(asString(request?.model)),
              },
              {
                label: "Streaming",
                value: formatPrimitive(asBoolean(request?.stream)),
              },
              {
                label: "Output limit",
                value: formatPrimitive(asNumber(request?.max_output_tokens)),
              },
              ...(toolNames.length > 0
                ? [
                    {
                      label: "Enabled tools",
                      value: renderToolBadges(toolNames),
                    },
                  ]
                : []),
            ]}
          />

          <SystemPromptSection prompts={prompts} />
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function ResponsesMessagesPanel({ request }: { request: JsonRecord | null }) {
  const input = asArray(request?.input)

  return (
    <MessageListSection
      items={input}
      title="Input timeline"
      description="Ordered provider input items."
    />
  )
}

function ChatCompletionsRequestContextCard({
  request,
  summary,
}: {
  request: JsonRecord | null
  summary: JsonRecord | null
}) {
  const messages = asArray(request?.messages)
  const messageSummary = summarizeChatMessages(messages)
  const toolNames = asArray(summary?.tool_names).filter(
    (value): value is string => typeof value === "string"
  )

  const prompts = messages
    .map((message, index) => {
      const record = asRecord(message)
      if (!record || asString(record.role) !== "system") return null

      const content = extractContent(record)
      if (!content) return null

      return {
        label: `system #${index + 1}`,
        source: `messages[${index}]`,
        content,
      }
    })
    .filter((value): value is PromptEntry => value != null)

  return (
    <Card size="sm">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="space-y-1">
            <CardTitle className="text-sm">Provider request</CardTitle>
            <CardDescription>
              Request summary and prompt context.
            </CardDescription>
          </div>
          <Badge variant="outline" className="text-[11px]">
            chat completions
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <DetailList
          items={[
            {
              label: "Model",
              value: formatPrimitive(asString(request?.model)),
            },
            {
              label: "Streaming",
              value: formatPrimitive(asBoolean(request?.stream)),
            },
            {
              label: "Output limit",
              value: formatPrimitive(asNumber(request?.max_completion_tokens)),
            },
            { label: "Messages", value: formatPrimitive(messages.length) },
            { label: "System", value: formatPrimitive(messageSummary.system) },
            { label: "User", value: formatPrimitive(messageSummary.user) },
            {
              label: "Assistant",
              value: formatPrimitive(messageSummary.assistant),
            },
            { label: "Tool", value: formatPrimitive(messageSummary.tool) },
            {
              label: "Assistant tool call blocks",
              value: formatPrimitive(messageSummary.assistantToolCalls),
            },
            { label: "Tool schemas", value: formatPrimitive(toolNames.length) },
            ...(toolNames.length > 0
              ? [{ label: "Enabled tools", value: renderToolBadges(toolNames) }]
              : []),
          ]}
        />

        <SystemPromptSection prompts={prompts} />
      </CardContent>
    </Card>
  )
}

function ChatCompletionsMessagesPanel({
  request,
}: {
  request: JsonRecord | null
}) {
  const messages = asArray(request?.messages)

  return (
    <MessageListSection
      items={messages}
      title="Conversation messages"
      description="Ordered messages sent upstream."
    />
  )
}

function ProviderRequestContextCard({
  protocol,
  request,
  summary,
}: {
  protocol: string
  request: JsonRecord | null
  summary: JsonRecord | null
}) {
  if (protocol === "openai-responses") {
    return <ResponsesRequestContextCard request={request} summary={summary} />
  }

  if (protocol === "openai-chat-completions") {
    return (
      <ChatCompletionsRequestContextCard request={request} summary={summary} />
    )
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">Provider request</CardTitle>
        <CardDescription>
          No protocol-specific parser available.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <RawJsonSection title="Full provider request" value={request} />
        <RawJsonSection title="Request summary" value={summary} />
      </CardContent>
    </Card>
  )
}

function ProviderRequestMessagesPanel({
  protocol,
  request,
}: {
  protocol: string
  request: JsonRecord | null
}) {
  if (protocol === "openai-responses") {
    return <ResponsesMessagesPanel request={request} />
  }

  if (protocol === "openai-chat-completions") {
    return <ChatCompletionsMessagesPanel request={request} />
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">Messages</CardTitle>
        <CardDescription>
          No structured message extraction available.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <RawJsonSection title="Provider request payload" value={request} />
      </CardContent>
    </Card>
  )
}

function ProviderRequestToolsPanel({
  protocol,
  request,
}: {
  protocol: string
  request: JsonRecord | null
}) {
  if (protocol === "openai-responses") {
    return <ToolListSection tools={asArray(request?.tools)} />
  }

  if (protocol === "openai-chat-completions") {
    return <ToolListSection tools={asArray(request?.tools)} />
  }

  return null
}

type PayloadTabKey = "request" | "messages" | "response" | "raw"

type PayloadTab = {
  key: PayloadTabKey
  label: string
  value: unknown
  kind: "json" | "text"
  badge?: string
  meta?: string
}

function resolveMessagePayload(trace: TraceRecord) {
  const request = asRecord(trace.provider_request)
  if (!request) return []

  if (trace.protocol === "openai-responses") {
    return asArray(request.input)
  }

  if (trace.protocol === "openai-chat-completions") {
    return asArray(request.messages)
  }

  const messages = asArray(request.messages)
  if (messages.length > 0) {
    return messages
  }

  return asArray(request.input)
}

function buildPayloadTabs(trace: TraceRecord): PayloadTab[] {
  const messagePayload = resolveMessagePayload(trace)
  const responseBody = trace.response_body
  const responseTab: PayloadTab = responseBody
    ? {
        key: "response",
        label: "Response",
        value: responseBody,
        kind: "text",
        badge: "raw",
        meta: `${responseBody.length} chars`,
      }
    : {
        key: "response",
        label: "Response",
        value: trace.response_summary,
        kind: "json",
        badge: "summary",
        meta: "response_summary",
      }

  return [
    {
      key: "request",
      label: "Request",
      value: trace.provider_request,
      kind: "json",
      badge: trace.protocol,
      meta: "provider_request",
    },
    {
      key: "messages",
      label: "Messages",
      value: messagePayload,
      kind: "json",
      badge: String(messagePayload.length),
      meta:
        trace.protocol === "openai-responses"
          ? "input[]"
          : trace.protocol === "openai-chat-completions"
            ? "messages[]"
            : "message payload",
    },
    responseTab,
    {
      key: "raw",
      label: "Raw",
      value: {
        request_summary: trace.request_summary,
        response_summary: trace.response_summary,
        response_body: trace.response_body,
      },
      kind: "json",
      meta: "summary bundle",
    },
  ]
}

function PayloadWorkbench({
  tabs,
  activeTab,
  onTabChange,
  copied,
  onCopyActive,
}: {
  tabs: PayloadTab[]
  activeTab: PayloadTabKey
  onTabChange: (tab: PayloadTabKey) => void
  copied: boolean
  onCopyActive: () => void
}) {
  const active = tabs.find((tab) => tab.key === activeTab) ?? tabs[0]

  if (!active) {
    return (
      <section className="rounded-xl border border-border/50 bg-background/85 p-4">
        <p className="text-[12px] text-muted-foreground">
          No payload available.
        </p>
      </section>
    )
  }

  return (
    <section className="overflow-hidden rounded-xl border border-border/50 bg-background/85">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/40 px-3 py-2.5">
        <div className="flex items-center gap-1">
          {tabs.map((tab) => {
            const activeTone = tab.key === activeTab
            return (
              <button
                key={tab.key}
                type="button"
                onClick={() => onTabChange(tab.key)}
                aria-pressed={activeTone}
                className={cn(
                  "inline-flex min-h-8 items-center gap-1.5 rounded-md border px-2.5 text-[11px] font-medium transition-colors",
                  activeTone
                    ? "border-foreground/20 bg-foreground text-background"
                    : "border-border/45 bg-background text-muted-foreground hover:text-foreground"
                )}
              >
                <span>{tab.label}</span>
                {tab.badge ? (
                  <span
                    className={cn(
                      "rounded-sm px-1 py-0.5 text-[10px]",
                      activeTone
                        ? "bg-background/20 text-background"
                        : "bg-muted/45 text-muted-foreground"
                    )}
                  >
                    {tab.badge}
                  </span>
                ) : null}
              </button>
            )
          })}
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={onCopyActive}
          className="h-8 px-2.5 text-[11px]"
        >
          {copied ? (
            <Check className="size-3.5" />
          ) : (
            <Copy className="size-3.5" />
          )}
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-border/30 bg-muted/[0.09] px-3 py-1.5 text-[11px] text-muted-foreground">
        <span className="font-medium text-foreground">{active.label}</span>
        {active.meta ? <span>{active.meta}</span> : null}
      </div>

      <div className="p-3">
        {active.key === "messages" ? (
          <MessageTimelineList
            items={asArray(active.value)}
            compact
            defaultOpenSystem={false}
          />
        ) : active.kind === "text" ? (
          <pre className="max-h-[min(70vh,740px)] overflow-auto rounded-lg border border-border/40 bg-background px-3 py-3 text-[12px] leading-5 whitespace-pre-wrap text-foreground">
            {payloadCopyText(active.value)}
          </pre>
        ) : (
          <pre className="max-h-[min(70vh,740px)] overflow-auto rounded-lg border border-border/40 bg-background px-3 py-3 text-[12px] leading-5 text-foreground">
            {formatJson(active.value)}
          </pre>
        )}
      </div>
    </section>
  )
}

function SummaryBadge({ label, value }: { label: string; value: string }) {
  return (
    <div className="inline-flex items-center gap-1.5 rounded-md bg-muted/30 px-2.5 py-1 text-[11px]">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium text-foreground">{value}</span>
    </div>
  )
}

function TraceSummaryBar({ trace }: { trace: TraceRecord }) {
  const failed = trace.status === "failed"
  const status = failed ? "failed" : "succeeded"
  const time = formatDateTime(trace.started_at_ms)

  return (
    <Card
      size="sm"
      className={cn(
        failed
          ? "border-destructive/25 bg-background/80"
          : "border-border/40 bg-background/80"
      )}
    >
      <CardHeader className="gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <CardTitle className="text-sm">Trace overview</CardTitle>
        </div>
        <CardDescription>Execution summary.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap gap-1.5">
          <SummaryBadge label="Model" value={trace.model} />
          <SummaryBadge label="Status" value={status} />
          <SummaryBadge
            label="Duration"
            value={trace.duration_ms != null ? `${trace.duration_ms} ms` : "—"}
          />
          <SummaryBadge
            label="Tokens"
            value={String(trace.total_tokens ?? 0)}
          />
          {trace.cached_tokens != null && trace.cached_tokens > 0 ? (
            <SummaryBadge label="Cached" value={String(trace.cached_tokens)} />
          ) : null}
          <SummaryBadge label="Time" value={time} />
        </div>

        <div className="text-[11px] text-muted-foreground">
          {trace.provider} · {trace.protocol} · {trace.request_kind} · step{" "}
          {trace.step_index}
        </div>

        <Collapsible className="rounded-lg border border-border/35 bg-muted/10 px-3 py-2">
          <CollapsibleTrigger className="flex w-full items-center justify-between gap-3 text-left">
            <span className="text-[12px] font-medium text-foreground">
              Details
            </span>
            <span className="text-[11px] text-muted-foreground">
              IDs & transport
            </span>
          </CollapsibleTrigger>
          <CollapsibleContent className="pt-2">
            <CompactDetailList
              items={[
                { label: "Trace ID", value: trace.id },
                { label: "Turn ID", value: trace.turn_id },
                { label: "Run ID", value: trace.run_id },
              ]}
            />
            <div className="mt-2 space-y-1 text-[11px] text-muted-foreground">
              <p>
                HTTP{" "}
                {trace.status_code != null ? String(trace.status_code) : "—"}
                {" · "}
                stop {trace.stop_reason ?? "—"}
                {" · "}
                stream {trace.streaming ? "yes" : "no"}
              </p>
              {trace.cached_tokens != null && trace.cached_tokens > 0 ? (
                <p>{`cached ${trace.cached_tokens} / total ${trace.total_tokens ?? 0} input tokens`}</p>
              ) : null}
              <p className="break-all">{`${trace.base_url}${trace.endpoint_path}`}</p>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
    </Card>
  )
}

function ResultSection({ trace }: { trace: TraceRecord }) {
  const failed = trace.status === "failed"
  const responseSummary = asRecord(trace.response_summary)
  const assistantText = asString(responseSummary?.assistant_text)
  const thinkingText = asString(responseSummary?.thinking_text)

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">Result</CardTitle>
        <CardDescription>Outcome and failure details.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {failed ? (
          <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-4">
            <div className="mb-2 flex items-center gap-2 text-sm font-medium text-destructive">
              <AlertTriangle className="size-4" />
              Failure detail
            </div>
            <TextBlock
              value={trace.error}
              className="max-h-[220px] bg-destructive/5 text-destructive"
            />
            {trace.response_body ? (
              <>
                <Separator className="my-3 opacity-40" />
                <p className="mb-2 text-[11px] font-medium tracking-wide text-destructive/75 uppercase">
                  Upstream response body
                </p>
                <TextBlock
                  value={trace.response_body}
                  className="max-h-[260px] bg-destructive/5"
                />
              </>
            ) : null}
          </div>
        ) : (
          <div className="trace-accent-surface rounded-xl p-4">
            <div className="mb-2 flex items-center gap-2 text-sm font-medium text-foreground">
              <CheckCircle2 className="size-4 text-[var(--trace-accent-strong)]" />
              Assistant text
            </div>
            <TextBlock value={assistantText} className="max-h-[280px]" />
          </div>
        )}

        {thinkingText && (
          <SectionPanel
            title="Completion metadata"
            description="Additional completion details."
          >
            <TextBlock value={thinkingText} className="max-h-[220px]" />
          </SectionPanel>
        )}
      </CardContent>
    </Card>
  )
}

function RawPayloadsCard({ trace }: { trace: TraceRecord }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">Payload snapshots</CardTitle>
        <CardDescription>Raw request/response blocks.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <RawJsonSection
          title="Provider request"
          value={trace.provider_request}
        />
        {trace.response_body ? (
          <Collapsible className="rounded-lg border border-border/50 bg-muted/15 px-3 py-2">
            <CollapsibleTrigger className="flex w-full items-center justify-between gap-3 text-left text-sm font-medium text-foreground">
              <span>Response body</span>
              <span className="text-xs text-muted-foreground">Raw text</span>
            </CollapsibleTrigger>
            <CollapsibleContent className="pt-3">
              <TextBlock
                value={trace.response_body}
                className="max-h-[420px]"
              />
            </CollapsibleContent>
          </Collapsible>
        ) : null}
        <RawJsonSection title="Request summary" value={trace.request_summary} />
        <RawJsonSection
          title="Response summary"
          value={trace.response_summary}
        />
      </CardContent>
    </Card>
  )
}

export function TraceDetailModal({
  open,
  trace,
  loading,
  onOpenChange,
}: {
  open: boolean
  trace: TraceRecord | null
  loading: boolean
  onOpenChange: (open: boolean) => void
}) {
  const payloadTabs = useMemo(
    () => (trace ? buildPayloadTabs(trace) : []),
    [trace]
  )
  const [activePayloadTab, setActivePayloadTab] =
    useState<PayloadTabKey>("request")
  const [copiedAction, setCopiedAction] = useState<
    "active" | "request" | "response" | null
  >(null)
  const copyTimerRef = useRef<number | null>(null)

  useEffect(() => {
    setActivePayloadTab("request")
  }, [trace?.id])

  useEffect(() => {
    return () => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current)
      }
    }
  }, [])

  const activePayload =
    payloadTabs.find((tab) => tab.key === activePayloadTab) ?? payloadTabs[0]
  const requestPayload = payloadTabs.find((tab) => tab.key === "request")
  const responsePayload = payloadTabs.find((tab) => tab.key === "response")

  const handleCopyPayload = async (
    value: unknown,
    action: "active" | "request" | "response"
  ) => {
    const success = await copyText(payloadCopyText(value))
    if (!success) return

    if (copyTimerRef.current !== null) {
      window.clearTimeout(copyTimerRef.current)
    }

    setCopiedAction(action)
    copyTimerRef.current = window.setTimeout(() => {
      setCopiedAction(null)
      copyTimerRef.current = null
    }, COPY_RESET_DELAY_MS)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPortal>
        <DialogBackdrop />
        <DialogPopup className="w-[min(1280px,calc(100vw-2rem))] max-w-[1280px]">
          <DialogHeader>
            <div className="min-w-0 space-y-2">
              <DialogTitle className="text-[15px]">
                {trace ? `${trace.model} · payload workbench` : "Trace detail"}
              </DialogTitle>
              <DialogDescription className="mt-0 text-[12px]">
                Payload-first diagnostics.
              </DialogDescription>
              {trace ? (
                <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
                  <Badge variant="outline" className="text-[10px]">
                    {trace.protocol}
                  </Badge>
                  <Badge
                    variant="outline"
                    className={cn(
                      "text-[10px]",
                      trace.status === "failed"
                        ? "border-destructive/40 text-destructive"
                        : "border-[var(--trace-accent-strong)]/40 text-[var(--trace-accent-strong)]"
                    )}
                  >
                    {trace.status}
                  </Badge>
                  <Badge variant="outline" className="text-[10px]">
                    step {trace.step_index}
                  </Badge>
                  <span className="text-muted-foreground">
                    {trace.provider}
                  </span>
                  <span className="text-muted-foreground">·</span>
                  <span className="text-muted-foreground">
                    {formatDateTime(trace.started_at_ms)}
                  </span>
                  <span className="text-muted-foreground">·</span>
                  <span className="text-muted-foreground tabular-nums">
                    {trace.duration_ms != null
                      ? `${trace.duration_ms} ms`
                      : "—"}
                  </span>
                </div>
              ) : null}
            </div>

            <div className="flex items-center gap-1.5">
              {trace ? (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 px-2.5 text-[11px]"
                    onClick={() => {
                      if (!requestPayload) return
                      void handleCopyPayload(requestPayload.value, "request")
                    }}
                  >
                    {copiedAction === "request" ? (
                      <Check className="size-3.5" />
                    ) : (
                      <Copy className="size-3.5" />
                    )}
                    {copiedAction === "request" ? "Copied" : "Request"}
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 px-2.5 text-[11px]"
                    onClick={() => {
                      if (!responsePayload) return
                      void handleCopyPayload(responsePayload.value, "response")
                    }}
                  >
                    {copiedAction === "response" ? (
                      <Check className="size-3.5" />
                    ) : (
                      <Copy className="size-3.5" />
                    )}
                    {copiedAction === "response" ? "Copied" : "Response"}
                  </Button>
                </>
              ) : null}
              <DialogClose />
            </div>
          </DialogHeader>
          <DialogBody>
            <ScrollArea className="h-[min(82vh,920px)] pr-4">
              {loading ? (
                <div className="flex min-h-[240px] items-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="size-4 animate-spin" />
                  Loading trace…
                </div>
              ) : null}

              {!loading && !trace ? (
                <div className="min-h-[240px] text-sm text-muted-foreground">
                  No trace selected.
                </div>
              ) : null}

              {trace ? (
                <div className="space-y-4 pb-1">
                  <TraceSummaryBar trace={trace} />

                  <div className="grid gap-4 xl:grid-cols-[minmax(0,1.35fr)_minmax(330px,0.65fr)]">
                    <PayloadWorkbench
                      tabs={payloadTabs}
                      activeTab={activePayloadTab}
                      onTabChange={setActivePayloadTab}
                      copied={copiedAction === "active"}
                      onCopyActive={() => {
                        if (!activePayload) return
                        void handleCopyPayload(activePayload.value, "active")
                      }}
                    />

                    <div className="space-y-4">
                      <ProviderRequestContextCard
                        protocol={trace.protocol}
                        request={trace.provider_request}
                        summary={trace.request_summary}
                      />
                      <ResultSection trace={trace} />
                    </div>
                  </div>

                  <div className="grid gap-4 xl:grid-cols-2">
                    <div className="space-y-4">
                      <ProviderRequestMessagesPanel
                        protocol={trace.protocol}
                        request={trace.provider_request}
                      />
                    </div>
                    <div className="space-y-4">
                      <ProviderRequestToolsPanel
                        protocol={trace.protocol}
                        request={trace.provider_request}
                      />
                    </div>
                  </div>

                  <RawPayloadsCard trace={trace} />
                </div>
              ) : null}
            </ScrollArea>
          </DialogBody>
        </DialogPopup>
      </DialogPortal>
    </Dialog>
  )
}
