import { memo, useEffect, useRef, useState } from "react"
import { Check, X as XIcon } from "lucide-react"
import { MarkdownContent } from "@/components/markdown-content"
import { Shimmer } from "@/components/ai-elements/shimmer"
import { getToolDisplayName, getToolDisplayPath } from "@/lib/tool-display"
import { useChatStore } from "@/stores/chat-store"
import type {
  StreamingToolOutput,
  StreamingTurn,
  ToolInvocationLifecycle,
  TurnBlock,
  TurnUsage,
  TurnLifecycle,
} from "@/lib/types"

const HISTORY_LOAD_TRIGGER_PX = 80

type ToolCategory = "read" | "search" | "edit" | "other"

const TOOL_CATEGORIES: Record<string, ToolCategory> = {
  read: "read",
  cat: "read",
  head: "read",
  tail: "read",
  grep: "search",
  search: "search",
  find: "search",
  glob: "search",
  ripgrep: "search",
  shell: "other",
  edit: "edit",
  write: "edit",
  apply_patch: "edit",
  replace: "edit",
  sed: "edit",
}

const CATEGORY_LABELS: Record<ToolCategory, string> = {
  read: "read",
  search: "search",
  edit: "edit",
  other: "tool",
}

function categorize(toolName: string): ToolCategory {
  return TOOL_CATEGORIES[toolName.toLowerCase()] ?? "other"
}

function getToolStats(details: Record<string, unknown> | undefined): {
  added?: number
  removed?: number
  lines?: number
  matches?: number
  returned?: number
  limit?: number
  truncated?: boolean
  linesRead?: number
  totalLines?: number
  exitCode?: number
} {
  if (!details) return {}
  return {
    added: typeof details.added === "number" ? details.added : undefined,
    removed: typeof details.removed === "number" ? details.removed : undefined,
    lines: typeof details.lines === "number" ? details.lines : undefined,
    matches: typeof details.matches === "number" ? details.matches : undefined,
    returned:
      typeof details.returned === "number" ? details.returned : undefined,
    limit: typeof details.limit === "number" ? details.limit : undefined,
    truncated:
      typeof details.truncated === "boolean" ? details.truncated : undefined,
    linesRead:
      typeof details.lines_read === "number" ? details.lines_read : undefined,
    totalLines:
      typeof details.total_lines === "number" ? details.total_lines : undefined,
    exitCode:
      typeof details.exit_code === "number" ? details.exit_code : undefined,
  }
}

function buildCategorySummary(
  invocations: { toolName: string }[]
): { category: ToolCategory; label: string; count: number }[] {
  const counts = new Map<ToolCategory, number>()
  for (const inv of invocations) {
    const cat = categorize(inv.toolName)
    counts.set(cat, (counts.get(cat) ?? 0) + 1)
  }
  return Array.from(counts.entries()).map(([cat, count]) => ({
    category: cat,
    label: CATEGORY_LABELS[cat],
    count,
  }))
}

type ToolRowItem = {
  id: string
  toolName: string
  arguments: Record<string, unknown>
  startedAtMs?: number
  finishedAtMs?: number
  succeeded: boolean
  outputContent: string
  details?: Record<string, unknown>
}

function fromInvocation(inv: ToolInvocationLifecycle): ToolRowItem {
  const { call, outcome } = inv
  if (outcome.status === "succeeded") {
    return {
      id: call.invocation_id,
      toolName: call.tool_name,
      arguments: call.arguments,
      startedAtMs: inv.started_at_ms,
      finishedAtMs: inv.finished_at_ms,
      succeeded: true,
      outputContent: outcome.result.content,
      details: outcome.result.details,
    }
  }
  return {
    id: call.invocation_id,
    toolName: call.tool_name,
    arguments: call.arguments,
    startedAtMs: inv.started_at_ms,
    finishedAtMs: inv.finished_at_ms,
    succeeded: false,
    outputContent: outcome.status === "failed" ? outcome.message : "",
  }
}

function fromStreamingTool(tool: StreamingToolOutput): ToolRowItem {
  return {
    id: tool.invocationId,
    toolName: tool.toolName,
    arguments: tool.arguments,
    startedAtMs: tool.startedAtMs ?? tool.detectedAtMs,
    finishedAtMs: tool.finishedAtMs,
    succeeded: !tool.failed,
    outputContent: tool.resultContent ?? tool.output,
    details: tool.resultDetails,
  }
}

function formatDurationMs(
  startedAtMs: number | undefined,
  finishedAtMs?: number
): string | null {
  if (!startedAtMs) return null
  const end = finishedAtMs ?? Date.now()
  const duration = Math.max(0, end - startedAtMs)
  if (duration < 1000) return `${duration} ms`
  if (duration < 60_000) return `${(duration / 1000).toFixed(1)} s`
  const minutes = Math.floor(duration / 60_000)
  const seconds = Math.floor((duration % 60_000) / 1000)
  return `${minutes}m ${seconds}s`
}

function formatScalar(value: unknown): string {
  if (typeof value === "string") return value
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value)
  }
  if (value == null) return "-"
  return JSON.stringify(value)
}

function getStreamingToolTarget(tool: StreamingToolOutput): string {
  const target = getToolDisplayPath(tool.toolName, undefined, tool.arguments)
  if (target) return target
  if (tool.startedAtMs) return "running"
  return "preparing"
}

function StructuredArguments({
  argumentsValue,
}: {
  argumentsValue: Record<string, unknown>
}) {
  const entries = Object.entries(argumentsValue)
  if (entries.length === 0) {
    return <p className="text-[12px] text-muted-foreground/60">No arguments</p>
  }

  const scalarEntries = entries.filter(([, value]) => {
    return (
      value == null ||
      typeof value === "string" ||
      typeof value === "number" ||
      typeof value === "boolean"
    )
  })
  const nestedEntries = entries.filter(([, value]) => {
    return !(
      value == null ||
      typeof value === "string" ||
      typeof value === "number" ||
      typeof value === "boolean"
    )
  })

  return (
    <div className="space-y-2">
      {scalarEntries.length > 0 ? (
        <dl className="divide-y divide-border/20 overflow-hidden rounded-md border border-border/30 bg-background/60">
          {scalarEntries.map(([label, value]) => (
            <div
              key={label}
              className="flex items-start justify-between gap-3 px-2.5 py-2"
            >
              <dt className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
                {label}
              </dt>
              <dd className="text-right text-[12px] leading-5 text-foreground/80">
                {formatScalar(value)}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
      {nestedEntries.map(([label, value]) => (
        <details
          key={label}
          className="overflow-hidden rounded-md border border-border/30 bg-background/60"
        >
          <summary className="cursor-pointer px-2.5 py-2 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {label}
          </summary>
          <div className="border-t border-border/20 px-2.5 py-2">
            <pre className="max-h-40 overflow-auto text-[11px] leading-5 text-muted-foreground/80">
              {JSON.stringify(value, null, 2)}
            </pre>
          </div>
        </details>
      ))}
    </div>
  )
}

function ExpandableOutput({
  value,
  failed,
}: {
  value: string
  failed: boolean
}) {
  const [open, setOpen] = useState(false)
  const needsCollapse = value.length > 280 || value.split("\n").length > 10

  return (
    <div className="space-y-2">
      <pre
        className={`overflow-auto rounded-md border p-2 text-[12px] leading-relaxed whitespace-pre-wrap ${
          failed
            ? "border-destructive/20 bg-destructive/[0.04] text-destructive/90"
            : "border-border/30 bg-background/60 text-muted-foreground/80"
        } ${!open && needsCollapse ? "max-h-44" : ""}`}
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

function ToolGroup({
  items,
  isStreaming = false,
}: {
  items: ToolRowItem[]
  isStreaming?: boolean
}) {
  const [open, setOpen] = useState(isStreaming)
  const allSucceeded = items.every((item) => item.succeeded)
  const summary = buildCategorySummary(items)

  useEffect(() => {
    if (isStreaming) {
      setOpen(true)
    }
  }, [isStreaming])

  return (
    <div className="mb-3">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <span className="font-medium">
          {isStreaming ? "Exploring" : "Explored"}
        </span>
        {!open && (
          <span className="text-muted-foreground/70">
            {summary
              .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
              .join(", ")}
          </span>
        )}
        {allSucceeded && <Check className="size-3.5 text-emerald-500/70" />}
      </button>
      {open && (
        <div className="mt-1 ml-5">
          {items.map((item) => (
            <ToolRow key={item.id} item={item} />
          ))}
        </div>
      )}
    </div>
  )
}

function ToolRow({ item }: { item: ToolRowItem }) {
  const [showDetails, setShowDetails] = useState(false)
  const stats = getToolStats(item.details)
  const displayPath = getToolDisplayPath(
    item.toolName,
    item.details,
    item.arguments
  )
  const duration = formatDurationMs(item.startedAtMs, item.finishedAtMs)

  return (
    <div>
      <button
        onClick={() => setShowDetails(!showDetails)}
        className="grid w-full grid-cols-[minmax(56px,max-content)_1fr_auto] items-center gap-x-2 py-0.5 text-[12px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <span className="truncate text-left font-medium text-muted-foreground/70">
          {getToolDisplayName(item.toolName)}
        </span>
        <span className="truncate text-left">{displayPath}</span>
        <div className="flex items-center gap-2">
          {duration && (
            <span className="shrink-0 text-muted-foreground/50">
              {duration}
            </span>
          )}
          {stats.added != null && (
            <span className="shrink-0 text-emerald-500">+{stats.added}</span>
          )}
          {stats.removed != null && (
            <span className="shrink-0 text-red-400">-{stats.removed}</span>
          )}
          {stats.lines != null && (
            <span className="shrink-0 text-emerald-500">+{stats.lines}</span>
          )}
          {stats.matches != null && !stats.truncated && (
            <span className="shrink-0 text-muted-foreground/50">
              {stats.matches} matches
            </span>
          )}
          {stats.truncated && stats.matches != null && (
            <span className="shrink-0 text-amber-600/80">
              {stats.matches} matches (showing {stats.returned})
            </span>
          )}
          {stats.linesRead != null && stats.totalLines != null && (
            <span className="shrink-0 text-muted-foreground/50">
              {stats.linesRead}/{stats.totalLines}
            </span>
          )}
          {item.succeeded ? (
            <Check className="size-3 shrink-0 text-foreground/30" />
          ) : (
            <XIcon className="size-3 shrink-0 text-destructive/70" />
          )}
        </div>
      </button>
      {showDetails && (
        <div className="mt-1 mb-2 ml-3 space-y-2.5 rounded-md border border-border/25 bg-muted/15 p-2">
          <div className="space-y-1.5">
            <p className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
              Arguments
            </p>
            <StructuredArguments argumentsValue={item.arguments} />
          </div>
          {item.outputContent ? (
            <div className="space-y-1.5">
              <p className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
                Outcome
              </p>
              <ExpandableOutput
                value={item.outputContent}
                failed={!item.succeeded}
              />
            </div>
          ) : null}
        </div>
      )}
    </div>
  )
}

function ThinkingBlock({
  content,
  isStreaming = false,
}: {
  content: string
  isStreaming?: boolean
}) {
  const [open, setOpen] = useState(isStreaming)
  const lastLine = content.trim().split("\n").pop() ?? ""

  return (
    <div className="mb-2">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
      >
        {isStreaming ? (
          <span className="font-medium">Thinking</span>
        ) : (
          <>
            <span className="font-medium">Thought</span>
            {!open && lastLine && (
              <span className="ml-1 max-w-[400px] truncate text-muted-foreground/50">
                {lastLine}
              </span>
            )}
          </>
        )}
      </button>
      {open && (
        <div className="mt-1.5 border-l-2 border-border/30 pl-3 text-[13px] leading-relaxed text-muted-foreground/80">
          <MarkdownContent content={content} />
        </div>
      )}
    </div>
  )
}

function StreamingToolGroup({
  toolOutputs,
}: {
  toolOutputs: StreamingToolOutput[]
}) {
  if (toolOutputs.length === 0) return null

  const completed = toolOutputs.filter((t) => t.completed)
  const active = toolOutputs.filter((t) => !t.completed)
  const activeSummary = buildCategorySummary(active)

  return (
    <div className="mb-2">
      {completed.length > 0 && (
        <ToolGroup items={completed.map(fromStreamingTool)} isStreaming />
      )}

      {active.length > 0 && (
        <>
          <div className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
            <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-amber-500/70" />
            <Shimmer as="span" className="font-medium" duration={2}>
              Exploring
            </Shimmer>
            <span className="text-muted-foreground/70">
              {activeSummary
                .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
                .join(", ")}
            </span>
          </div>
          <div className="mt-0.5 ml-3 space-y-0.5">
            {active.map((tool) => (
              <div
                key={tool.invocationId}
                className="grid grid-cols-[minmax(48px,max-content)_1fr_auto] items-center gap-x-2 py-0.5 text-[13px] text-muted-foreground/60"
              >
                {tool.toolName && (
                  <span className="truncate text-left font-medium">
                    {getToolDisplayName(tool.toolName)}
                  </span>
                )}
                <span className="truncate text-left">
                  {getStreamingToolTarget(tool)}
                </span>
                <span className="shrink-0 text-muted-foreground/50">
                  {tool.startedAtMs
                    ? formatDurationMs(tool.startedAtMs, tool.finishedAtMs) ??
                      "0 ms"
                    : "queued"}
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  )
}

type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: ToolInvocationLifecycle[] }

function groupBlocks(blocks: TurnBlock[]): BlockGroup[] {
  const result: BlockGroup[] = []

  for (const block of blocks) {
    if (block.kind === "tool_invocation") {
      const last = result[result.length - 1]
      if (last && last.type === "tools") {
        last.invocations.push(block.invocation)
      } else {
        result.push({ type: "tools", invocations: [block.invocation] })
      }
    } else {
      result.push({ type: "single", block })
    }
  }
  return result
}

function BlockRenderer({ block }: { block: TurnBlock }) {
  switch (block.kind) {
    case "thinking":
      return <ThinkingBlock content={block.content} />
    case "assistant":
      return (
        <MarkdownContent
          content={block.content}
          className="text-sm leading-[1.75] text-pretty"
        />
      )
    case "failure":
      return (
        <div className="mb-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[13px] text-destructive">
          {block.message}
        </div>
      )
    case "cancelled":
      return (
        <div className="mb-3 rounded-lg border border-border/40 bg-muted/40 px-3 py-2 text-[13px] text-muted-foreground">
          {block.message}
        </div>
      )
    case "tool_invocation":
      return null
  }
}

const STATUS_LABELS: Record<StreamingTurn["status"], string> = {
  waiting: "Waiting",
  thinking: "Thinking",
  working: "Working",
  generating: "Generating",
  cancelled: "Cancelled",
}

function StatusIndicator({ status }: { status: StreamingTurn["status"] }) {
  return (
    <div className="py-2">
      <Shimmer as="span" className="text-[14px] font-medium" duration={2}>
        {STATUS_LABELS[status]}
      </Shimmer>
    </div>
  )
}

function TurnUsageBadge({ usage }: { usage: TurnUsage }) {
  const cachedSuffix =
    usage.cached_tokens > 0
      ? ` · ${usage.cached_tokens.toLocaleString()} cached`
      : ""
  return (
    <span className="text-[11px] font-normal tracking-normal text-muted-foreground/70 normal-case">
      {`${usage.input_tokens.toLocaleString()} in · ${usage.output_tokens.toLocaleString()} out · ${usage.total_tokens.toLocaleString()} total tok${cachedSuffix}`}
    </span>
  )
}

function TurnMeta({ turn }: { turn: TurnLifecycle }) {
  const duration = formatDurationMs(turn.started_at_ms, turn.finished_at_ms)
  const statusLabel =
    turn.outcome === "cancelled"
      ? "cancelled"
      : turn.outcome === "failed"
        ? "failed"
        : null

  if (!duration && !turn.usage && !statusLabel) return null

  return (
    <div className="mt-2 flex items-center gap-3 text-[11px] text-muted-foreground/55 opacity-0 transition-opacity duration-150 group-hover/turn:opacity-100 group-focus-within/turn:opacity-100">
      {duration && (
        <span className="tabular-nums text-muted-foreground/65">{`latency ${duration}`}</span>
      )}
      {statusLabel && (
        <span className="rounded-full border border-border/40 px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/80">
          {statusLabel}
        </span>
      )}
      {turn.usage && (
        <span className="pointer-events-none">
          <TurnUsageBadge usage={turn.usage} />
        </span>
      )}
    </div>
  )
}

function UserMessageBlock({ content }: { content: string }) {
  return (
    <div className="border-l border-foreground/14 pl-4">
      <div className="max-w-[64ch] text-[14px] leading-[1.8] text-foreground/90">
        <MarkdownContent content={content} />
      </div>
    </div>
  )
}

function TurnView({ turn }: { turn: TurnLifecycle }) {
  const grouped = groupBlocks(turn.blocks)

  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both] last:mb-0">
      <div className="mb-6">
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold tracking-[0.1em] text-foreground/70 uppercase">
            You
          </span>
        </div>
        <UserMessageBlock content={turn.user_message} />
      </div>

      <div className="group/turn">
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold tracking-[0.1em] text-muted-foreground uppercase">
            aia
          </span>
        </div>
        {grouped.map((group, i) => {
          if (group.type === "tools") {
            return (
              <ToolGroup
                key={i}
                items={group.invocations.map(fromInvocation)}
              />
            )
          }
          return <BlockRenderer key={i} block={group.block} />
        })}
        <TurnMeta turn={turn} />
      </div>
    </div>
  )
}

type StreamingGroup =
  | { type: "thinking"; content: string }
  | { type: "text"; content: string }
  | { type: "tools"; tools: StreamingToolOutput[] }

function groupStreamingBlocks(
  blocks: StreamingTurn["blocks"]
): StreamingGroup[] {
  const groups: StreamingGroup[] = []
  for (const block of blocks) {
    if (block.type === "tool") {
      const last = groups[groups.length - 1]
      if (last && last.type === "tools") {
        last.tools.push(block.tool)
      } else {
        groups.push({ type: "tools", tools: [block.tool] })
      }
    } else {
      groups.push(block)
    }
  }
  return groups
}

function StreamingView({ streaming }: { streaming: StreamingTurn }) {
  const groups = groupStreamingBlocks(streaming.blocks)

  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both]">
      {streaming.userMessage && (
        <div className="mb-6">
          <div className="mb-2 flex items-baseline gap-2.5">
            <span className="text-[11px] font-semibold tracking-[0.1em] text-foreground/70 uppercase">
              You
            </span>
          </div>
          <UserMessageBlock content={streaming.userMessage} />
        </div>
      )}

      <div>
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold tracking-[0.1em] text-muted-foreground uppercase">
            aia
          </span>
        </div>
        {groups.map((group, i) => {
          if (group.type === "thinking") {
            const isLast =
              i === groups.length - 1 && streaming.status === "thinking"
            return (
              <ThinkingBlock
                key={i}
                content={group.content}
                isStreaming={isLast}
              />
            )
          }
          if (group.type === "tools") {
            return <StreamingToolGroup key={i} toolOutputs={group.tools} />
          }
          return (
            <MarkdownContent
              key={i}
              content={group.content}
              streaming
              className="text-sm leading-[1.75] text-pretty"
            />
          )
        })}
      </div>
    </div>
  )
}

const MemoizedTurnView = memo(
  TurnView,
  (prevProps, nextProps) => prevProps.turn === nextProps.turn
)
const MemoizedStreamingView = memo(
  StreamingView,
  (prevProps, nextProps) => prevProps.streaming === nextProps.streaming
)

function CompressionNotice({ summary }: { summary: string }) {
  return (
    <div className="mb-4 rounded-lg border border-border/30 bg-muted/25 px-3 py-2 text-[12px] text-muted-foreground">
      <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-foreground/60">
        Context compressed
      </div>
      <p className="line-clamp-3 whitespace-pre-wrap">{summary}</p>
    </div>
  )
}

function SessionHydratingIndicator() {
  return (
    <div className="pointer-events-none sticky top-0 z-10 mb-3">
      <div className="mx-auto flex w-fit items-center gap-2 rounded-full border border-border/40 bg-background/90 px-3 py-1.5 text-[12px] text-muted-foreground shadow-sm backdrop-blur-sm">
        <span className="size-1.5 animate-pulse rounded-full bg-foreground/40" />
        <span>Loading session…</span>
      </div>
    </div>
  )
}

export function ChatMessages() {
  const turns = useChatStore((s) => s.turns)
  const sessionHydrating = useChatStore((s) => s.sessionHydrating)
  const historyHasMore = useChatStore((s) => s.historyHasMore)
  const historyLoadingMore = useChatStore((s) => s.historyLoadingMore)
  const loadOlderTurns = useChatStore((s) => s.loadOlderTurns)
  const streamingTurn = useChatStore((s) => s.streamingTurn)
  const error = useChatStore((s) => s.error)
  const lastCompression = useChatStore((s) => s.lastCompression)
  const activeSessionId = useChatStore((s) => s.activeSessionId)
  const containerRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const historyTriggerRef = useRef<HTMLDivElement>(null)
  const previousSessionIdRef = useRef<string | null>(null)
  const previousTurnCountRef = useRef(0)
  const previousStreamingBlockCountRef = useRef(0)
  const shouldStickToBottomRef = useRef(true)
  const restoreSessionScrollRef = useRef(false)
  const skipNextAutoScrollRef = useRef(false)
  const scrollPositionsRef = useRef<Record<string, number>>({})
  const autoLoadingOlderTurnsRef = useRef(false)
  const historyHasMoreRef = useRef(historyHasMore)
  const historyLoadingMoreRef = useRef(historyLoadingMore)
  const sessionHydratingRef = useRef(sessionHydrating)
  const [scrollTop, setScrollTop] = useState(0)
  const [, setContainerHeight] = useState(0)

  const visibleTurns = turns
  const topSpacerHeight = 0
  const bottomSpacerHeight = 0
  const showHistoryHint = historyLoadingMore || scrollTop < 160

  useEffect(() => {
    historyHasMoreRef.current = historyHasMore
    historyLoadingMoreRef.current = historyLoadingMore
    sessionHydratingRef.current = sessionHydrating
  }, [historyHasMore, historyLoadingMore, sessionHydrating])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const handleScroll = () => {
      const distanceFromBottom =
        container.scrollHeight - container.scrollTop - container.clientHeight
      shouldStickToBottomRef.current = distanceFromBottom < 120
      setScrollTop(container.scrollTop)
      if (activeSessionId) {
        scrollPositionsRef.current[activeSessionId] = container.scrollTop
      }
    }

    const resizeObserver = new ResizeObserver(() => {
      setContainerHeight(container.clientHeight)
    })

    setContainerHeight(container.clientHeight)
    handleScroll()
    container.addEventListener("scroll", handleScroll)
    resizeObserver.observe(container)
    return () => {
      container.removeEventListener("scroll", handleScroll)
      resizeObserver.disconnect()
    }
  }, [activeSessionId])

  useEffect(() => {
    const container = containerRef.current
    const historyTrigger = historyTriggerRef.current
    if (!container || !historyTrigger) {
      return
    }

    if (typeof IntersectionObserver === "undefined") {
      if (container.scrollTop <= HISTORY_LOAD_TRIGGER_PX) {
        void handleLoadOlderTurns()
      }
      return
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0]
        if (!entry?.isIntersecting) return
        void handleLoadOlderTurns()
      },
      {
        root: container,
        rootMargin: "80px 0px 0px 0px",
      }
    )

    observer.observe(historyTrigger)
    return () => {
      observer.disconnect()
    }
  }, [activeSessionId, turns.length, historyHasMore, historyLoadingMore, sessionHydrating])

  useEffect(() => {
    const previousSessionId = previousSessionIdRef.current
    if (previousSessionId && previousSessionId !== activeSessionId) {
      restoreSessionScrollRef.current = true
      shouldStickToBottomRef.current = true
      skipNextAutoScrollRef.current = false
    }
  }, [activeSessionId])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    if (restoreSessionScrollRef.current) {
      requestAnimationFrame(() => {
        const nextContainer = containerRef.current
        if (!nextContainer) return
        bottomRef.current?.scrollIntoView({ behavior: "auto" })
        setScrollTop(nextContainer.scrollTop)
        if (activeSessionId) {
          scrollPositionsRef.current[activeSessionId] = nextContainer.scrollTop
        }
        restoreSessionScrollRef.current = false
      })
      previousSessionIdRef.current = activeSessionId
      previousTurnCountRef.current = turns.length
      previousStreamingBlockCountRef.current = streamingTurn?.blocks.length ?? 0
      return
    }

    if (skipNextAutoScrollRef.current) {
      skipNextAutoScrollRef.current = false
      previousSessionIdRef.current = activeSessionId
      previousTurnCountRef.current = turns.length
      previousStreamingBlockCountRef.current = streamingTurn?.blocks.length ?? 0
      return
    }

    const currentStreamingBlockCount = streamingTurn?.blocks.length ?? 0
    const sessionChanged = previousSessionIdRef.current !== activeSessionId
    const hydratedManyTurns = turns.length > previousTurnCountRef.current + 1
    const hydratedStreamingSnapshot =
      currentStreamingBlockCount > previousStreamingBlockCountRef.current + 1

    const shouldAutoScroll =
      sessionChanged ||
      hydratedManyTurns ||
      hydratedStreamingSnapshot ||
      shouldStickToBottomRef.current

    if (!shouldAutoScroll) {
      previousSessionIdRef.current = activeSessionId
      previousTurnCountRef.current = turns.length
      previousStreamingBlockCountRef.current = currentStreamingBlockCount
      return
    }

    const behavior: ScrollBehavior =
      sessionChanged || hydratedManyTurns || hydratedStreamingSnapshot
        ? "auto"
        : "smooth"

    bottomRef.current?.scrollIntoView({ behavior })

    previousSessionIdRef.current = activeSessionId
    previousTurnCountRef.current = turns.length
    previousStreamingBlockCountRef.current = currentStreamingBlockCount
  }, [activeSessionId, turns.length, streamingTurn?.blocks.length])

  async function handleLoadOlderTurns() {
    if (
      autoLoadingOlderTurnsRef.current ||
      historyLoadingMoreRef.current ||
      sessionHydratingRef.current ||
      !historyHasMoreRef.current
    ) {
      return
    }

    autoLoadingOlderTurnsRef.current = true
    const container = containerRef.current
    const previousScrollHeight = container?.scrollHeight ?? 0
    skipNextAutoScrollRef.current = true
    try {
      await loadOlderTurns()
      requestAnimationFrame(() => {
        const nextContainer = containerRef.current
        if (!nextContainer) {
          autoLoadingOlderTurnsRef.current = false
          return
        }
        const nextScrollHeight = nextContainer.scrollHeight
        nextContainer.scrollTop += nextScrollHeight - previousScrollHeight
        setScrollTop(nextContainer.scrollTop)
        if (activeSessionId) {
          scrollPositionsRef.current[activeSessionId] = nextContainer.scrollTop
        }
        autoLoadingOlderTurnsRef.current = false
      })
    } catch {
      autoLoadingOlderTurnsRef.current = false
    }
  }

  if (turns.length === 0 && !streamingTurn) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center px-4">
        <h2 className="font-serif text-3xl font-semibold tracking-tight text-foreground">
          What can I help with?
        </h2>
        <p className="mt-2.5 text-sm text-muted-foreground">
          Start a conversation or ask anything.
        </p>
        {error && (
          <p className="mt-4 max-w-md text-center text-sm text-destructive">
            {error}
          </p>
        )}
      </div>
    )
  }

  return (
    <div ref={containerRef} className="relative flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[720px] px-6 py-8">
        {sessionHydrating && <SessionHydratingIndicator />}
        {historyHasMore && (
          <>
            <div ref={historyTriggerRef} className="h-px w-full" aria-hidden="true" />
            <div
              className={showHistoryHint
                ? "sticky top-0 z-10 -mx-6 mb-4 flex justify-center bg-gradient-to-b from-background via-background/95 to-transparent px-6 pt-2 pb-3 opacity-100 transition-opacity duration-150 pointer-events-none"
                : "sticky top-0 z-10 -mx-6 mb-4 flex justify-center bg-gradient-to-b from-background via-background/95 to-transparent px-6 pt-2 pb-3 opacity-0 transition-opacity duration-150 pointer-events-none"
              }
              aria-hidden={!showHistoryHint}
            >
              <div className="rounded-full border border-border/30 bg-background/70 px-2.5 py-1 text-[11px] text-muted-foreground/85 shadow-sm backdrop-blur-sm">
                {historyLoadingMore
                  ? "Loading older messages…"
                  : "Scroll up for older messages"}
              </div>
            </div>
          </>
        )}
        <div
          className={sessionHydrating ? "transition-opacity duration-150 ease-out opacity-80" : "transition-opacity duration-150 ease-out opacity-100"}
          aria-busy={sessionHydrating}
        >
          {topSpacerHeight > 0 && <div style={{ height: topSpacerHeight }} />}
          {visibleTurns.map((turn) => (
            <MemoizedTurnView key={turn.turn_id} turn={turn} />
          ))}
          {bottomSpacerHeight > 0 && <div style={{ height: bottomSpacerHeight }} />}
          {lastCompression && !streamingTurn && (
            <CompressionNotice summary={lastCompression.summary} />
          )}
          {streamingTurn && <MemoizedStreamingView streaming={streamingTurn} />}
          {error && (
            <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[13px] text-destructive">
              {error}
            </div>
          )}
        </div>
        <div ref={bottomRef} />
      </div>
      {streamingTurn && (
        <div className="sticky bottom-0 z-10 bg-gradient-to-t from-background via-background to-transparent pt-6 pb-4">
          <div className="mx-auto max-w-[720px] px-6">
            <StatusIndicator status={streamingTurn.status} />
          </div>
        </div>
      )}
    </div>
  )
}
