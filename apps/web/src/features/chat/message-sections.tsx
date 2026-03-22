import { memo, useState } from "react"

import { Shimmer } from "@/components/ai-elements/shimmer"
import { MarkdownContent } from "@/components/markdown-content"
import type {
  StreamingToolOutput,
  StreamingTurn,
  TurnBlock,
  TurnLifecycle,
  TurnUsage,
} from "@/lib/types"

import { MemoizedStreamingToolGroup, MemoizedToolGroup } from "./tool-timeline"
import {
  formatDurationMs,
  fromInvocation,
} from "@/features/chat/tool-timeline-helpers.ts"

type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: TurnLifecycle["tool_invocations"] }

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

function groupStreamingBlocks(blocks: StreamingTurn["blocks"]) {
  const groups: Array<
    | { type: "thinking"; content: string }
    | { type: "text"; content: string }
    | { type: "tools"; tools: StreamingToolOutput[] }
  > = []

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
        className="flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
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
        <div className="mt-1.5 border-l-2 border-border/30 pl-3 text-xs leading-relaxed text-muted-foreground/80">
          <MarkdownContent content={content} />
        </div>
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
        <MarkdownContent
          content={block.content}
          className="max-w-[66ch] text-sm leading-[1.75] text-pretty"
        />
      )
    case "failure":
      return (
        <div className="mb-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs leading-relaxed font-medium text-destructive">
          {block.message}
        </div>
      )
    case "cancelled":
      return (
        <div className="mb-3 rounded-lg border border-border/40 bg-muted/40 px-3 py-2 text-xs leading-relaxed font-medium text-muted-foreground">
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
  finishing: "Wrapping up",
  cancelled: "Cancelled",
}

export function StatusIndicator({
  status,
}: {
  status: StreamingTurn["status"]
}) {
  return (
    <div className="py-2" role="status" aria-live="polite" aria-atomic="true">
      <Shimmer as="span" className="text-sm font-medium" duration={2}>
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
    <span className="text-xs font-normal tracking-normal text-muted-foreground/70 normal-case">
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
    <div className="mt-2 flex items-center gap-3 text-xs text-muted-foreground/55 opacity-0 transition-opacity duration-150 group-focus-within/turn:opacity-100 group-hover/turn:opacity-100">
      {duration && (
        <span className="text-muted-foreground/65 tabular-nums">{`latency ${duration}`}</span>
      )}
      {statusLabel && (
        <span className="rounded-full border border-border/40 px-2 py-0.5 text-[0.6875rem] font-medium tracking-[0.08em] text-muted-foreground/80 uppercase">
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

export function UserMessageBlock({ content }: { content: string }) {
  return (
    <div className="border-l border-foreground/14 pl-4">
      <div className="max-w-[66ch] text-sm leading-[1.8] text-pretty text-foreground/90">
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
          <span className="text-[0.6875rem] font-semibold tracking-[0.1em] text-foreground/70 uppercase">
            You
          </span>
        </div>
        <UserMessageBlock content={turn.user_message} />
      </div>

      <div className="group/turn">
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[0.6875rem] font-semibold tracking-[0.1em] text-muted-foreground uppercase">
            aia
          </span>
        </div>
        {grouped.map((group, i) => {
          if (group.type === "tools") {
            return (
              <MemoizedToolGroup
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

function StreamingView({ streaming }: { streaming: StreamingTurn }) {
  const groups = groupStreamingBlocks(streaming.blocks)

  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both]">
      {streaming.userMessage && (
        <div className="mb-6">
          <div className="mb-2 flex items-baseline gap-2.5">
            <span className="text-[0.6875rem] font-semibold tracking-[0.1em] text-foreground/70 uppercase">
              You
            </span>
          </div>
          <UserMessageBlock content={streaming.userMessage} />
        </div>
      )}

      <div>
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="text-[0.6875rem] font-semibold tracking-[0.1em] text-muted-foreground uppercase">
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
            return (
              <MemoizedStreamingToolGroup key={i} toolOutputs={group.tools} />
            )
          }
          return (
            <MarkdownContent
              key={i}
              content={group.content}
              streaming
              className="max-w-[66ch] text-sm leading-[1.75] text-pretty"
            />
          )
        })}
      </div>
    </div>
  )
}

export const MemoizedTurnView = memo(
  TurnView,
  (prevProps, nextProps) => prevProps.turn === nextProps.turn
)

export const MemoizedStreamingView = memo(
  StreamingView,
  (prevProps, nextProps) => prevProps.streaming === nextProps.streaming
)

export function CompressionNotice({ summary }: { summary: string }) {
  return (
    <div className="mb-4 rounded-lg border border-border/30 bg-muted/25 px-3 py-2 text-xs text-muted-foreground">
      <div className="mb-1 text-[0.6875rem] font-semibold tracking-[0.08em] text-foreground/60 uppercase">
        Context compressed
      </div>
      <p className="line-clamp-3 whitespace-pre-wrap">{summary}</p>
    </div>
  )
}

export function SessionHydratingIndicator({
  reducedMotion = false,
}: {
  reducedMotion?: boolean
}) {
  return (
    <div className="pointer-events-none sticky top-0 z-10 mb-3">
      <div
        className="mx-auto flex w-fit max-w-full items-center gap-2 rounded-full border border-border/35 bg-background/88 px-3 py-1.5 text-xs text-muted-foreground/80 shadow-none"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        <span
          className={
            reducedMotion
              ? "size-1.5 rounded-full bg-foreground/35"
              : "size-1.5 animate-pulse rounded-full bg-foreground/35"
          }
        />
        <span>Loading session…</span>
      </div>
    </div>
  )
}
