import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react"
import type { ReactNode } from "react"

import { Badge } from "@/components/ui/badge"
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
import type { TraceRecord } from "@/lib/types"
import { cn } from "@/lib/utils"

type JsonRecord = Record<string, unknown>

type PromptEntry = {
  label: string
  source: string
  content: string
}

function isRecord(value: unknown): value is JsonRecord {
  return value != null && typeof value === "object" && !Array.isArray(value)
}

function asRecord(value: unknown): JsonRecord | null {
  return isRecord(value) ? value : null
}

function asString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null
}

function asNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null
}

function asBoolean(value: unknown): boolean | null {
  return typeof value === "boolean" ? value : null
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
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

function truncate(text: string, max: number) {
  return text.length > max ? `${text.slice(0, max)}…` : text
}

function extractText(value: unknown): string {
  if (typeof value === "string") return value

  if (Array.isArray(value)) {
    return value
      .map((item) => extractText(item))
      .filter(Boolean)
      .join("\n")
      .trim()
  }

  const record = asRecord(value)
  if (!record) return ""

  for (const key of ["text", "summary_text", "content", "output", "value"]) {
    const text = extractText(record[key])
    if (text) return text
  }

  return Object.values(record)
    .map((item) => extractText(item))
    .filter(Boolean)
    .join(" ")
    .trim()
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
  switch (kind) {
    case "system":
      return "border-sky-500/30 bg-sky-500/5"
    case "user":
      return "border-amber-500/30 bg-amber-500/5"
    case "assistant":
      return "border-emerald-500/25 bg-emerald-500/5"
    case "tool":
    case "function_call":
    case "function_call_output":
      return "border-violet-500/25 bg-violet-500/5"
    default:
      return "border-border/50 bg-muted/15"
  }
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

  return extractText(item.content) || extractText(item.output) || ""
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
    <dl className="grid gap-x-4 gap-y-3 sm:grid-cols-2">
      {items.map((item) => (
        <div
          key={item.label}
          className="space-y-1 rounded-lg bg-muted/25 px-3 py-2"
        >
          <dt className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {item.label}
          </dt>
          <dd className="text-[13px] break-all text-foreground">
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
    <dl className="grid gap-x-6 gap-y-2 sm:grid-cols-2">
      {items.map((item) => (
        <div key={item.label} className="flex items-baseline gap-2">
          <dt className="shrink-0 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {item.label}
          </dt>
          <dd className="min-w-0 text-[13px] break-all text-foreground">
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
  description: string
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
          <p className="text-[12px] text-muted-foreground">{description}</p>
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
      description="Instruction text and system-role messages that shape the request before user content and tool activity."
      className="border-sky-500/20 bg-sky-500/[0.04]"
      meta={
        <>
          <Badge variant="outline" className="text-[11px]">
            {prompts.length} prompt{prompts.length === 1 ? "" : "s"}
          </Badge>
          <Badge variant="secondary" className="text-[11px]">
            {totalCharacters} chars
          </Badge>
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
              className="rounded-xl border border-sky-500/20 bg-background/70 p-3"
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
                className="max-h-[320px] bg-sky-500/[0.04]"
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
      description="Schemas exposed to the provider, including required parameters and per-tool descriptions."
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

  return (
    <SectionPanel
      title={title}
      description={description}
      meta={counts.map(([kind, count]) => (
        <Badge key={kind} variant="outline" className="text-[11px]">
          {kind}: {count}
        </Badge>
      ))}
    >
      {items.length === 0 ? (
        <p className="text-[13px] text-muted-foreground">No items.</p>
      ) : (
        <div className="space-y-2">
          {items.map((item, index) => {
            const record = asRecord(item)
            if (!record) {
              return (
                <div
                  key={index}
                  className="rounded-xl border border-border/50 bg-background/70 px-4 py-3 text-[12px] text-muted-foreground"
                >
                  {typeof item === "string"
                    ? truncate(item, 180)
                    : `Item ${index + 1}`}
                </div>
              )
            }

            const kind = normalizeMessageKind(record)
            const preview = extractContent(record)
            const toolName =
              asString(record.name) ?? asString(asRecord(record.function)?.name)
            const toolCalls = asArray(record.tool_calls)
            const callId =
              asString(record.call_id) ?? asString(record.tool_call_id)

            return (
              <Collapsible
                key={`${kind}-${index}`}
                defaultOpen={kind === "system"}
                className={cn(
                  "rounded-xl border bg-background/80",
                  messageToneClasses(kind)
                )}
              >
                <CollapsibleTrigger className="flex w-full flex-wrap items-start justify-between gap-3 px-4 py-3 text-left">
                  <div className="min-w-0 space-y-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="outline" className="text-[10px]">
                        #{index + 1}
                      </Badge>
                      <Badge
                        variant={roleBadgeVariant(kind)}
                        className="text-[11px]"
                      >
                        {kind}
                      </Badge>
                      {toolName ? (
                        <Badge variant="secondary" className="text-[11px]">
                          {toolName}
                        </Badge>
                      ) : null}
                      {callId ? (
                        <Badge variant="outline" className="text-[10px]">
                          call {callId}
                        </Badge>
                      ) : null}
                      {toolCalls.length > 0 ? (
                        <Badge variant="outline" className="text-[10px]">
                          {toolCalls.length} tool call
                          {toolCalls.length === 1 ? "" : "s"}
                        </Badge>
                      ) : null}
                    </div>
                    <p className="text-[12px] leading-5 text-foreground/85">
                      {preview
                        ? truncate(preview, 220)
                        : "No preview available."}
                    </p>
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    expand
                  </span>
                </CollapsibleTrigger>
                <CollapsibleContent className="border-t border-border/30 px-4 py-3">
                  <pre className="max-h-[360px] overflow-auto rounded-lg bg-muted/35 p-3 text-[12px] leading-6 whitespace-pre-wrap text-foreground">
                    {formatJson(record)}
                  </pre>
                </CollapsibleContent>
              </Collapsible>
            )
          })}
        </div>
      )}
    </SectionPanel>
  )
}

function summarizeResponsesInput(input: unknown[]) {
  let messages = 0
  let functionCalls = 0
  let functionOutputs = 0

  for (const item of input) {
    const record = asRecord(item)
    const kind = asString(record?.type)
    if (kind === "function_call") {
      functionCalls += 1
    } else if (kind === "function_call_output") {
      functionOutputs += 1
    } else {
      messages += 1
    }
  }

  return { messages, functionCalls, functionOutputs }
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
  const tools = asArray(request?.tools)
  const reasoning = asRecord(request?.reasoning)
  const inputSummary = summarizeResponsesInput(input)
  const resumeCheckpoint = asString(summary?.resume_checkpoint)
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
    <Card size="sm">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="space-y-1">
            <CardTitle className="text-sm">Provider request</CardTitle>
            <CardDescription>
              OpenAI Responses payload reorganized around instructions, input
              items, and tool schemas.
            </CardDescription>
          </div>
          <Badge variant="outline" className="text-[11px]">
            responses
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
              label: "Output limit",
              value: formatPrimitive(asNumber(request?.max_output_tokens)),
            },
            {
              label: "Previous response",
              value: formatPrimitive(asString(request?.previous_response_id)),
            },
            { label: "Input items", value: formatPrimitive(input.length) },
            {
              label: "Message items",
              value: formatPrimitive(inputSummary.messages),
            },
            {
              label: "Function calls",
              value: formatPrimitive(inputSummary.functionCalls),
            },
            {
              label: "Function outputs",
              value: formatPrimitive(inputSummary.functionOutputs),
            },
            { label: "Tool schemas", value: formatPrimitive(tools.length) },
            {
              label: "Reasoning effort",
              value: formatPrimitive(asString(reasoning?.effort)),
            },
            {
              label: "Streaming",
              value: formatPrimitive(asBoolean(request?.stream)),
            },
            ...(resumeCheckpoint
              ? [{ label: "Resume checkpoint", value: resumeCheckpoint }]
              : []),
            ...(toolNames.length > 0
              ? [{ label: "Enabled tools", value: renderToolBadges(toolNames) }]
              : []),
          ].slice(0, 6)}
        />

        <SystemPromptSection prompts={prompts} />
      </CardContent>
    </Card>
  )
}

function ResponsesMessagesPanel({ request }: { request: JsonRecord | null }) {
  const input = asArray(request?.input)

  return (
    <MessageListSection
      items={input}
      title="Input timeline"
      description="Every input item sent to the Responses API, including messages, function calls, and function outputs."
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
  const resumeCheckpoint = asString(summary?.resume_checkpoint)
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
              Chat Completions payload reorganized around system messages,
              conversation order, and tool definitions.
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
            {
              label: "Streaming",
              value: formatPrimitive(asBoolean(request?.stream)),
            },
            { label: "Messages", value: formatPrimitive(messages.length) },
            {
              label: "Assistant tool call blocks",
              value: formatPrimitive(messageSummary.assistantToolCalls),
            },
            ...(resumeCheckpoint
              ? [{ label: "Resume checkpoint", value: resumeCheckpoint }]
              : []),
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
      description="Ordered chat messages as sent upstream, including assistant tool call envelopes and tool-role outputs."
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
          No specialized parser is defined for this protocol yet.
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
          No structured message extraction is defined for this protocol yet.
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

function SummaryBadge({
  label,
  value,
  variant = "outline",
}: {
  label: string
  value: string
  variant?:
    | "default"
    | "secondary"
    | "destructive"
    | "outline"
    | "ghost"
    | "link"
}) {
  return (
    <div className="rounded-full border border-border/50 bg-background/80 px-3 py-1.5 text-[11px]">
      <span className="mr-1 text-muted-foreground">{label}</span>
      <Badge variant={variant} className="h-auto px-1.5 py-0 text-[10px]">
        {value}
      </Badge>
    </div>
  )
}

function TraceSummaryBar({ trace }: { trace: TraceRecord }) {
  const failed = trace.status === "failed"

  return (
    <Card
      size="sm"
      className={cn(
        failed
          ? "border-destructive/30 bg-destructive/[0.03]"
          : "border-emerald-500/20 bg-emerald-500/[0.04]"
      )}
    >
      <CardHeader className="gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <CardTitle className="text-sm">Trace overview</CardTitle>
        </div>
        <CardDescription>
          One-line summary of transport state, execution result, and stable
          request identifiers.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-2">
          <SummaryBadge
            label="HTTP"
            value={trace.status_code != null ? String(trace.status_code) : "—"}
            variant={
              trace.status_code != null && trace.status_code >= 400
                ? "destructive"
                : "outline"
            }
          />
          <SummaryBadge label="Model" value={trace.model} />
          <SummaryBadge label="Step" value={String(trace.step_index)} />
          <SummaryBadge
            label="Duration"
            value={trace.duration_ms != null ? `${trace.duration_ms} ms` : "—"}
          />
          <SummaryBadge label="Stop" value={trace.stop_reason ?? "—"} />
          <SummaryBadge
            label="Tokens"
            value={String(trace.total_tokens ?? 0)}
          />
          <SummaryBadge label="Stream" value={trace.streaming ? "yes" : "no"} />
        </div>
        <CompactDetailList
          items={[
            { label: "Trace ID", value: trace.id },
            { label: "Turn ID", value: trace.turn_id },
            { label: "Run ID", value: trace.run_id },
            { label: "Provider", value: trace.provider },
            { label: "Protocol", value: trace.protocol },
            {
              label: "Endpoint",
              value: `${trace.base_url}${trace.endpoint_path}`,
            },
            {
              label: "Request",
              value: `${trace.request_kind} · step ${trace.step_index}`,
            },
            { label: "Started", value: formatDateTime(trace.started_at_ms) },
          ]}
        />
      </CardContent>
    </Card>
  )
}

function ResultSection({ trace }: { trace: TraceRecord }) {
  const failed = trace.status === "failed"
  const responseSummary = asRecord(trace.response_summary)
  const assistantText = asString(responseSummary?.assistant_text)
  const thinkingText = asString(responseSummary?.thinking_text)
  const checkpoint = asString(responseSummary?.checkpoint)

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">Result</CardTitle>
        <CardDescription>
          Final outcome details only. Success shows extracted assistant output;
          failure shows the captured error path.
        </CardDescription>
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
          <div className="rounded-xl border border-emerald-500/20 bg-background/80 p-4">
            <div className="mb-2 flex items-center gap-2 text-sm font-medium text-foreground">
              <CheckCircle2 className="size-4 text-emerald-600" />
              Assistant text
            </div>
            <TextBlock value={assistantText} className="max-h-[280px]" />
          </div>
        )}

        {(thinkingText || checkpoint || trace.checkpoint_out) && (
          <SectionPanel
            title="Completion metadata"
            description="Only the remaining result details that are not already in the top summary."
          >
            <DetailList
              items={[
                {
                  label: "Checkpoint out",
                  value: formatPrimitive(trace.checkpoint_out ?? checkpoint),
                },
              ]}
            />
            {thinkingText ? (
              <>
                <Separator className="my-4 opacity-40" />
                <TextBlock value={thinkingText} className="max-h-[220px]" />
              </>
            ) : null}
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
        <CardTitle className="text-sm">Raw payloads</CardTitle>
        <CardDescription>
          Fallback inspection only. Open these when the structured sections are
          not enough.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <RawJsonSection
          title="Provider request"
          value={trace.provider_request}
        />
        <RawJsonSection title="Request summary" value={trace.request_summary} />
        <RawJsonSection
          title="Response summary"
          value={trace.response_summary}
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
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPortal>
        <DialogBackdrop />
        <DialogPopup className="w-[min(1280px,calc(100vw-2rem))] max-w-[1280px]">
          <DialogHeader>
            <div className="min-w-0 space-y-1">
              <DialogTitle>
                {trace
                  ? `${trace.model} · step ${trace.step_index}`
                  : "Trace detail"}
              </DialogTitle>
              <DialogDescription>
                Diagnostic view focused on prompt context, message ordering,
                tool definitions, and execution outcome.
              </DialogDescription>
            </div>
            <DialogClose />
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
                <div className="space-y-5 pb-1">
                  <div className="grid gap-5 xl:grid-cols-[minmax(0,1.2fr)_minmax(360px,0.8fr)]">
                    <div className="space-y-5">
                      <TraceSummaryBar trace={trace} />
                      <ProviderRequestContextCard
                        protocol={trace.protocol}
                        request={trace.provider_request}
                        summary={trace.request_summary}
                      />
                      <ProviderRequestToolsPanel
                        protocol={trace.protocol}
                        request={trace.provider_request}
                      />
                      <ResultSection trace={trace} />
                      <RawPayloadsCard trace={trace} />
                    </div>

                    <div className="space-y-5">
                      <ProviderRequestMessagesPanel
                        protocol={trace.protocol}
                        request={trace.provider_request}
                      />
                    </div>
                  </div>
                </div>
              ) : null}
            </ScrollArea>
          </DialogBody>
        </DialogPopup>
      </DialogPortal>
    </Dialog>
  )
}
