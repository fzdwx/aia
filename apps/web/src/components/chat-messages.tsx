import { Fragment, useEffect, useRef, useState } from "react"
import { ChevronDown } from "lucide-react"
import { cn } from "@/lib/utils"
import type {
  StreamingTurn,
  TurnBlock,
  TurnLifecycle,
} from "@/lib/types"

type ChatMessagesProps = {
  turns: TurnLifecycle[]
  streamingTurn: StreamingTurn | null
  error: string | null
}

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

function ThinkingBlock({ content }: { content: string }) {
  const [open, setOpen] = useState(false)

  return (
    <div className="mb-3 rounded-lg border border-border/30 bg-muted/30">
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 px-3 py-2 text-[12px] font-medium text-muted-foreground transition-colors hover:text-foreground"
      >
        <ChevronDown
          className={cn(
            "size-3.5 transition-transform",
            !open && "-rotate-90",
          )}
        />
        Thinking
      </button>
      {open && (
        <div className="border-t border-border/20 px-3 py-2 text-[13px] leading-relaxed text-muted-foreground/80">
          <MarkdownContent content={content} />
        </div>
      )}
    </div>
  )
}

function ToolInvocationBlock({
  block,
}: {
  block: Extract<TurnBlock, { kind: "tool_invocation" }>
}) {
  const { call, outcome } = block.invocation
  const succeeded = outcome.status === "succeeded"

  return (
    <div className="mb-3 rounded-lg border border-border/30 bg-muted/30 px-3 py-2">
      <div className="flex items-center gap-2 text-[12px] font-medium text-muted-foreground">
        <span
          className={cn(
            "size-1.5 rounded-full",
            succeeded ? "bg-green-500" : "bg-destructive",
          )}
        />
        <span className="font-mono">{call.tool_name}</span>
      </div>
      {succeeded && (
        <pre className="mt-1.5 max-h-[120px] overflow-auto text-[12px] leading-relaxed text-muted-foreground/70">
          {outcome.result.content}
        </pre>
      )}
      {!succeeded && outcome.status === "failed" && (
        <p className="mt-1.5 text-[12px] text-destructive/80">
          {outcome.message}
        </p>
      )}
    </div>
  )
}

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

function TurnView({ turn }: { turn: TurnLifecycle }) {
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
        {turn.blocks.map((block, i) => (
          <BlockRenderer key={i} block={block} />
        ))}
      </div>
    </div>
  )
}

const STATUS_LABELS: Record<StreamingTurn["status"], string> = {
  waiting: "Waiting",
  thinking: "Thinking",
  working: "Working",
  generating: "Generating",
}

function StatusIndicator({ status }: { status: StreamingTurn["status"] }) {
  return (
    <div className="py-1">
      <span className="shimmer-text text-[12px] font-medium tracking-wide">
        {STATUS_LABELS[status]}
      </span>
    </div>
  )
}

function StreamingToolBlock({
  invocationId,
  output,
}: {
  invocationId: string
  output: string
}) {
  return (
    <div className="mb-3 rounded-lg border border-border/30 bg-muted/30 px-3 py-2">
      <div className="flex items-center gap-2 text-[12px] font-medium text-muted-foreground">
        <span className="size-1.5 rounded-full bg-amber-500/70 animate-pulse" />
        <span className="font-mono">{invocationId}</span>
      </div>
      {output && (
        <pre className="mt-1.5 max-h-[120px] overflow-auto text-[12px] leading-relaxed text-muted-foreground/70">
          {output}
        </pre>
      )}
    </div>
  )
}

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
        {streaming.toolOutputs.map((tool) => (
          <StreamingToolBlock
            key={tool.invocationId}
            invocationId={tool.invocationId}
            output={tool.output}
          />
        ))}
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

export function ChatMessages({
  turns,
  streamingTurn,
  error,
}: ChatMessagesProps) {
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
