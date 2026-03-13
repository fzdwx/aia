import { Fragment, useEffect, useRef, useState } from "react"
import { Check, ChevronRight, X as XIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import { Shimmer } from "@/components/ai-elements/shimmer"
import { useChatStore } from "@/stores/chat-store"
import type {
  StreamingTurn,
  TurnBlock,
  TurnLifecycle,
} from "@/lib/types"

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

// --- Thinking block: compact collapsible label ---

function ThinkingBlock({ content }: { content: string }) {
  const [open, setOpen] = useState(false)

  return (
    <div className="mb-2">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <ChevronRight
          className={cn(
            "size-3.5 transition-transform",
            open && "rotate-90",
          )}
        />
        <span className="font-medium">Thought</span>
      </button>
      {open && (
        <div className="mt-1.5 ml-5 border-l-2 border-border/30 pl-3 text-[13px] leading-relaxed text-muted-foreground/80">
          <MarkdownContent content={content} />
        </div>
      )}
    </div>
  )
}

// --- Tool invocation: flat indented row ---

function ToolInvocationBlock({
  block,
}: {
  block: Extract<TurnBlock, { kind: "tool_invocation" }>
}) {
  const { call, outcome } = block.invocation
  const succeeded = outcome.status === "succeeded"

  // Derive a friendly label from tool name + first arg
  const firstArg = Object.values(call.arguments)[0]
  const argLabel = typeof firstArg === "string" ? firstArg : null
  const label = argLabel
    ? `${call.tool_name} ${argLabel}`
    : call.tool_name

  return (
    <div className="flex items-center gap-2 py-0.5 pl-5 text-[13px] text-muted-foreground">
      <span>{label}</span>
      {succeeded && (
        <Check className="size-3.5 text-foreground/50" />
      )}
      {!succeeded && outcome.status === "failed" && (
        <XIcon className="size-3.5 text-destructive/70" />
      )}
    </div>
  )
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
    case "tool_invocation":
      return <ToolInvocationBlock block={block} />
    case "failure":
      return (
        <div className="mb-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[13px] text-destructive">
          {block.message}
        </div>
      )
  }
}

// --- Group consecutive tool blocks with a summary header ---

function groupBlocks(blocks: TurnBlock[]) {
  const groups: { type: "single"; block: TurnBlock }[] | { type: "tools"; blocks: TurnBlock[] }[] = []
  type Group = { type: "single"; block: TurnBlock } | { type: "tools"; blocks: TurnBlock[] }
  const result: Group[] = []

  for (const block of blocks) {
    if (block.kind === "tool_invocation") {
      const last = result[result.length - 1]
      if (last && last.type === "tools") {
        last.blocks.push(block)
      } else {
        result.push({ type: "tools", blocks: [block] })
      }
    } else {
      result.push({ type: "single", block })
    }
  }
  void groups
  return result
}

function ToolGroupHeader({ count }: { count: number }) {
  return (
    <div className="mb-0.5 text-[13px] text-muted-foreground">
      <span>Ran {count} tool{count > 1 ? "s" : ""}</span>
    </div>
  )
}

// --- Turn view ---

function TurnView({ turn }: { turn: TurnLifecycle }) {
  const grouped = groupBlocks(turn.blocks)

  return (
    <div className="mb-8 last:mb-0 animate-[message-in_250ms_ease-out_both]">
      {/* User message */}
      <div className="mb-6">
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-foreground/70">
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
          <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            aia
          </span>
        </div>
        {grouped.map((group, i) => {
          if (group.type === "single") {
            return <BlockRenderer key={i} block={group.block} />
          }
          return (
            <div key={i} className="mb-3">
              <ToolGroupHeader count={group.blocks.length} />
              {group.blocks.map((block, j) => (
                <BlockRenderer key={j} block={block} />
              ))}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// --- Status indicator (more prominent) ---

const STATUS_LABELS: Record<StreamingTurn["status"], string> = {
  waiting: "Waiting",
  thinking: "Thinking",
  working: "Working",
  generating: "Generating",
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

// --- Streaming tool block: flat row with pulse dot ---

function StreamingToolBlock({
  invocationId,
  output,
}: {
  invocationId: string
  output: string
}) {
  return (
    <div className="flex items-center gap-2 py-0.5 pl-5 text-[13px] text-muted-foreground">
      <span className="size-1.5 rounded-full bg-amber-500/70 animate-pulse" />
      <span>{invocationId}</span>
      {output && (
        <span className="truncate text-muted-foreground/50">{output.split("\n")[0]}</span>
      )}
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
            <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-foreground/70">
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
          <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            aia
          </span>
        </div>
        {streaming.thinkingText && (
          <ThinkingBlock content={streaming.thinkingText} />
        )}
        {streaming.toolOutputs.length > 0 && (
          <div className="mb-2">
            {streaming.toolOutputs.map((tool) => (
              <StreamingToolBlock
                key={tool.invocationId}
                invocationId={tool.invocationId}
                output={tool.output}
              />
            ))}
          </div>
        )}
        {streaming.assistantText ? (
          <div className="text-[14px] leading-[1.75] text-foreground/85">
            <MarkdownContent content={streaming.assistantText} />
          </div>
        ) : null}
        <StatusIndicator status={streaming.status} />
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
  }, [turns.length, streamingTurn?.assistantText, streamingTurn?.toolOutputs.length])

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
    <div className="flex-1 overflow-y-auto">
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
    </div>
  )
}
