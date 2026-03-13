import { Fragment, useEffect, useRef, useState } from "react"
import { Check, X as XIcon } from "lucide-react"
import { Shimmer } from "@/components/ai-elements/shimmer"
import { useChatStore } from "@/stores/chat-store"
import type {
  StreamingToolOutput,
  StreamingTurn,
  ToolInvocationLifecycle,
  TurnBlock,
  TurnLifecycle,
} from "@/lib/types"

// --- Markdown rendering ---

function renderInline(text: string) {
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g)
  return parts.map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) {
      return (
        <strong key={i} className="font-semibold text-foreground">
          {part.slice(2, -2)}
        </strong>
      )
    }
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          className="rounded-[4px] bg-muted px-1.5 py-0.5 font-mono text-[0.85em]"
        >
          {part.slice(1, -1)}
        </code>
      )
    }
    return <Fragment key={i}>{part}</Fragment>
  })
}

function MarkdownContent({ content }: { content: string }) {
  const segments = content.split(/(```[\s\S]*?```)/g)

  return (
    <>
      {segments.map((segment, i) => {
        if (segment.startsWith("```")) {
          const inner = segment.slice(3, -3)
          const firstNewline = inner.indexOf("\n")
          const code = firstNewline >= 0 ? inner.slice(firstNewline + 1) : inner
          return (
            <pre
              key={i}
              className="my-3 overflow-x-auto rounded-lg border border-border/40 bg-muted/60 p-4 font-mono text-[13px] leading-relaxed"
            >
              <code>{code}</code>
            </pre>
          )
        }

        const paragraphs = segment.split(/\n\n+/)
        return paragraphs.map((para, j) => {
          if (!para.trim()) return null
          const lines = para.split("\n")
          return (
            <p key={`${i}-${j}`} className="mb-2.5 last:mb-0">
              {lines.map((line, k) => (
                <Fragment key={k}>
                  {k > 0 && <br />}
                  {renderInline(line)}
                </Fragment>
              ))}
            </p>
          )
        })
      })}
    </>
  )
}

// --- Tool categorization & labeling ---

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
  patch: "edit",
  replace: "edit",
  sed: "edit",
}

const CATEGORY_LABELS: Record<ToolCategory, string> = {
  read: "read",
  search: "search",
  edit: "edit",
  other: "tool use",
}

function categorize(toolName: string): ToolCategory {
  return TOOL_CATEGORIES[toolName.toLowerCase()] ?? "other"
}

/** Extract display label from tool details (backend-provided) or fallback to first arg */
function getToolDisplayPath(
  details: Record<string, unknown> | undefined,
  args: Record<string, unknown>
): string {
  if (details) {
    if (typeof details.file_path === "string") return details.file_path
    if (typeof details.pattern === "string") return details.pattern
    if (typeof details.command === "string") return details.command
  }
  // fallback: first string arg
  const firstStr = Object.values(args).find((v) => typeof v === "string") as
    | string
    | undefined
  return firstStr ?? ""
}

/** Extract diff/line stats from backend details */
function getToolStats(details: Record<string, unknown> | undefined): {
  added?: number
  removed?: number
  lines?: number
  matches?: number
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

// --- Thinking display ---

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

// --- Completed tool group: collapsible with categorized summary ---

function ToolGroupView({
  invocations,
}: {
  invocations: ToolInvocationLifecycle[]
}) {
  const [open, setOpen] = useState(false)
  const allSucceeded = invocations.every(
    (inv) => inv.outcome.status === "succeeded"
  )
  const summary = buildCategorySummary(
    invocations.map((inv) => ({ toolName: inv.call.tool_name }))
  )

  return (
    <div className="mb-3">
      {/* Summary header */}
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <span className="font-medium">Explored</span>
        <span className="text-muted-foreground/70">
          {summary
            .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
            .join(", ")}
        </span>
        {allSucceeded && <Check className="size-3.5 text-emerald-500/70" />}
      </button>

      {/* Expanded tool details */}
      {open && (
        <div className="mt-1 ml-5">
          {invocations.map((inv) => (
            <ToolInvocationRow key={inv.call.invocation_id} invocation={inv} />
          ))}
        </div>
      )}
    </div>
  )
}

function ToolInvocationRow({
  invocation,
}: {
  invocation: ToolInvocationLifecycle
}) {
  const [showOutput, setShowOutput] = useState(false)
  const { call, outcome } = invocation
  const succeeded = outcome.status === "succeeded"
  const outputContent = succeeded
    ? outcome.result.content
    : outcome.status === "failed"
      ? outcome.message
      : ""

  const details = succeeded ? outcome.result.details : undefined
  const stats = getToolStats(details)
  const displayPath = getToolDisplayPath(details, call.arguments)

  return (
    <div>
      <button
        onClick={() => setShowOutput(!showOutput)}
        className="flex w-full items-center gap-2 py-0.5 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <span className="shrink-0 font-medium text-muted-foreground/70">
          {call.tool_name}
        </span>
        <span className="truncate">{displayPath}</span>
        {stats.added != null && (
          <span className="shrink-0 text-emerald-500">+{stats.added}</span>
        )}
        {stats.removed != null && (
          <span className="shrink-0 text-red-400">-{stats.removed}</span>
        )}
        {stats.lines != null && (
          <span className="shrink-0 text-emerald-500">+{stats.lines}</span>
        )}
        {stats.matches != null && (
          <span className="shrink-0 text-muted-foreground/50">
            {stats.matches} matches
          </span>
        )}
        {stats.linesRead != null && stats.totalLines != null && (
          <span className="shrink-0 text-muted-foreground/50">
            {stats.linesRead}/{stats.totalLines}
          </span>
        )}
        {succeeded ? (
          <Check className="size-3 shrink-0 text-foreground/30" />
        ) : (
          <XIcon className="size-3 shrink-0 text-destructive/70" />
        )}
      </button>
      {showOutput && outputContent && (
        <pre className="mt-0.5 mb-1 ml-5 max-h-[300px] overflow-auto rounded border border-border/40 bg-muted/40 p-2 font-mono text-[12px] leading-relaxed text-muted-foreground/80">
          {outputContent}
        </pre>
      )}
    </div>
  )
}

// --- Streaming tool group ---

function StreamingToolGroup({
  toolOutputs,
}: {
  toolOutputs: StreamingToolOutput[]
}) {
  if (toolOutputs.length === 0) return null

  const summary = buildCategorySummary(toolOutputs)

  return (
    <div className="mb-2">
      <div className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
        <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-amber-500/70" />
        <Shimmer as="span" className="font-medium" duration={2}>
          Exploring
        </Shimmer>
        <span className="text-muted-foreground/70">
          {summary
            .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
            .join(", ")}
        </span>
      </div>
      {/* Show current active tool */}
      {toolOutputs.length > 0 && (
        <div className="mt-0.5 ml-5">
          {toolOutputs.map((tool) => (
            <div
              key={tool.invocationId}
              className="flex items-center gap-2 py-0.5 text-[13px] text-muted-foreground/60"
            >
              {tool.toolName && (
                <span className="shrink-0 font-medium">{tool.toolName}</span>
              )}
              <span className="truncate">
                {getToolDisplayPath(undefined, tool.arguments) ||
                  tool.invocationId}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// --- Block grouping ---

type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: ToolInvocationLifecycle[] }
  | { type: "thinking"; content: string }

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
    } else if (block.kind === "thinking") {
      const last = result[result.length - 1]
      if (last && last.type === "thinking") {
        last.content += "\n" + block.content
      } else {
        result.push({ type: "thinking", content: block.content })
      }
    } else {
      result.push({ type: "single", block })
    }
  }
  return result
}

// --- Block renderer ---

function BlockRenderer({ block }: { block: TurnBlock }) {
  switch (block.kind) {
    case "thinking":
      return <ThinkingBlock content={block.content} />
    case "assistant":
      return (
        <div className="text-[14px] leading-[1.75] text-foreground/85">
          <MarkdownContent content={block.content} />
        </div>
      )
    case "failure":
      return (
        <div className="mb-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[13px] text-destructive">
          {block.message}
        </div>
      )
    case "tool_invocation":
      // Handled by grouping — should not reach here in normal flow
      return null
  }
}

// --- Status indicator ---

const STATUS_LABELS: Record<StreamingTurn["status"], string> = {
  waiting: "Waiting",
  thinking: "Thinking",
  working: "Working",
  generating: "Generating",
}

function StatusIndicator({ status }: { status: StreamingTurn["status"] }) {
  // Working status has its own UI (StreamingToolGroup)
  if (status === "working") return null

  return (
    <div className="py-2">
      <Shimmer as="span" className="text-[14px] font-medium" duration={2}>
        {STATUS_LABELS[status]}
      </Shimmer>
    </div>
  )
}

// --- Turn view ---

function TurnView({ turn }: { turn: TurnLifecycle }) {
  const grouped = groupBlocks(turn.blocks)

  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both] last:mb-0">
      {/* User message */}
      <div className="mb-6">
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold tracking-[0.1em] text-foreground/70 uppercase">
            You
          </span>
        </div>
        <div className="text-[14px] leading-[1.75] text-foreground/85">
          <MarkdownContent content={turn.user_message} />
        </div>
      </div>

      {/* Assistant response blocks */}
      <div>
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold tracking-[0.1em] text-muted-foreground uppercase">
            aia
          </span>
        </div>
        {grouped.map((group, i) => {
          if (group.type === "tools") {
            return <ToolGroupView key={i} invocations={group.invocations} />
          }
          return <BlockRenderer key={i} block={group.block} />
        })}
      </div>
    </div>
  )
}

// --- Streaming view ---

function StreamingView({ streaming }: { streaming: StreamingTurn }) {
  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both]">
      {/* User message */}
      {streaming.userMessage && (
        <div className="mb-6">
          <div className="mb-2 flex items-baseline gap-2.5">
            <span className="text-[11px] font-semibold tracking-[0.1em] text-foreground/70 uppercase">
              You
            </span>
          </div>
          <div className="text-[14px] leading-[1.75] text-foreground/85">
            <MarkdownContent content={streaming.userMessage} />
          </div>
        </div>
      )}

      {/* Assistant response */}
      <div>
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold tracking-[0.1em] text-muted-foreground uppercase">
            aia
          </span>
        </div>
        {streaming.thinkingText && (
          <ThinkingBlock
            content={streaming.thinkingText}
            isStreaming={streaming.status === "thinking"}
          />
        )}
        <StreamingToolGroup toolOutputs={streaming.toolOutputs} />
        {streaming.assistantText ? (
          <div className="text-[14px] leading-[1.75] text-foreground/85">
            <MarkdownContent content={streaming.assistantText} />
          </div>
        ) : null}
      </div>
    </div>
  )
}

export function ChatMessages() {
  const turns = useChatStore((s) => s.turns)
  const streamingTurn = useChatStore((s) => s.streamingTurn)
  const error = useChatStore((s) => s.error)
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [
    turns.length,
    streamingTurn?.assistantText,
    streamingTurn?.toolOutputs.length,
  ])

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
    <div className="relative flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[720px] px-6 py-8">
        {turns.map((turn) => (
          <TurnView key={turn.turn_id} turn={turn} />
        ))}
        {streamingTurn && <StreamingView streaming={streamingTurn} />}
        {error && (
          <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[13px] text-destructive">
            {error}
          </div>
        )}
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
