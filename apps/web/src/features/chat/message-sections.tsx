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
  isContextExplorationTool,
} from "@/features/chat/tool-timeline-helpers.ts"

const CHAT_TURN_LABEL = "workspace-section-label text-foreground/70"
const CHAT_TURN_META = "workspace-section-label text-muted-foreground"

type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: TurnLifecycle["tool_invocations"] }

function groupBlocks(blocks: TurnBlock[]): BlockGroup[] {
  const result: BlockGroup[] = []

  for (const block of blocks) {
    if (block.kind === "tool_invocation") {
      const last = result[result.length - 1]
      const isContextTool = isContextExplorationTool(
        block.invocation.call.tool_name
      )
      const lastInvocation =
        last && last.type === "tools"
          ? last.invocations[last.invocations.length - 1]
          : null
      const canAppendToContextGroup =
        lastInvocation != null &&
        isContextTool &&
        isContextExplorationTool(lastInvocation.call.tool_name)

      if (canAppendToContextGroup && last && last.type === "tools") {
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
      const isContextTool = isContextExplorationTool(block.tool.toolName)
      const lastTool =
        last && last.type === "tools" ? last.tools[last.tools.length - 1] : null
      const canAppendToContextGroup =
        lastTool != null &&
        isContextTool &&
        isContextExplorationTool(lastTool.toolName)

      if (canAppendToContextGroup && last && last.type === "tools") {
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
        className="text-body-sm flex items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground"
      >
        {isStreaming ? (
          <span className="font-semibold">Thinking</span>
        ) : (
          <>
            <span className="font-semibold">Thought</span>
            {!open && lastLine && (
              <span className="ml-1 max-w-[400px] truncate text-muted-foreground/50">
                {lastLine}
              </span>
            )}
          </>
        )}
      </button>
      {open && (
        <div className="text-body-sm leading-body-sm mt-1.5 border-l-2 border-border/30 pl-3 text-muted-foreground/80">
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
          className="text-body-sm leading-body-sm max-w-[66ch] text-pretty"
        />
      )
    case "failure":
      return (
        <div className="text-caption mb-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 font-medium text-destructive">
          {block.message}
        </div>
      )
    case "cancelled":
      return (
        <div className="text-caption mb-3 rounded-lg border border-border/40 bg-muted/40 px-3 py-2 font-medium text-muted-foreground">
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
      <Shimmer as="span" duration={4} spread={6}>
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
    <span className="text-caption font-normal tracking-normal text-muted-foreground/70 normal-case">
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
    <div className="text-caption mt-2 flex items-center gap-3 text-muted-foreground/55 opacity-0 transition-opacity duration-150 group-focus-within/turn:opacity-100 group-hover/turn:opacity-100">
      {duration && (
        <span className="text-muted-foreground/65 tabular-nums">{`latency ${duration}`}</span>
      )}
      {statusLabel && (
        <span className="text-label rounded-full border border-border/40 px-2 py-0.5 font-medium tracking-[0.08em] text-muted-foreground/80">
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
      <div className="text-body-sm leading-body-sm max-w-[66ch] text-pretty text-foreground/90">
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
          <span className="workspace-section-label text-foreground/70">
            You
          </span>
        </div>
        <UserMessageBlock content={turn.user_message} />
      </div>

      <div className="group/turn">
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className="workspace-section-label text-muted-foreground">
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
            <span className={CHAT_TURN_LABEL}>You</span>
          </div>
          <UserMessageBlock content={streaming.userMessage} />
        </div>
      )}

      <div>
        <div className="mb-2 flex items-baseline gap-2.5">
          <span className={CHAT_TURN_META}>aia</span>
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
              className="text-body-sm leading-body-sm max-w-[66ch] text-pretty"
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
    <div className="text-caption mb-4 rounded-lg border border-border/30 bg-muted/25 px-3 py-2 text-muted-foreground">
      <div className="workspace-section-label mb-1 text-foreground/60">
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
        className="text-caption mx-auto flex w-fit max-w-full items-center gap-2 rounded-full border border-border/35 bg-background/88 px-3 py-1.5 text-muted-foreground/80 shadow-none"
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
