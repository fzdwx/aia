import { Check, Copy } from "lucide-react"
import { memo, useEffect, useRef, useState } from "react"

import { Shimmer } from "@/components/ai-elements/shimmer"
import { MarkdownContent } from "@/components/markdown-content"
import { copyTextToClipboard } from "@/lib/clipboard"
import type {
  StreamingToolOutput,
  StreamingTurn,
  TurnBlock,
  TurnLifecycle,
  TurnUsage,
} from "@/lib/types"
import { cn } from "@/lib/utils"

import { MemoizedStreamingToolGroup, MemoizedToolGroup } from "./tool-timeline"
import {
  formatDurationMs,
  fromInvocation,
  isContextExplorationTool,
} from "@/features/chat/tool-timeline-helpers.ts"

const COPY_RESET_DELAY_MS = 1500
const MESSAGE_READING_MEASURE = "w-full"

type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: TurnLifecycle["tool_invocations"] }

type StreamingBlockGroup =
  | { type: "thinking"; content: string }
  | { type: "text"; content: string }
  | { type: "tools"; tools: StreamingToolOutput[] }

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
  const groups: StreamingBlockGroup[] = []

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

function MessageCopyButton({
  content,
  copyLabel,
  copiedLabel,
  className,
}: {
  content: string
  copyLabel: string
  copiedLabel: string
  className?: string
}) {
  const [copied, setCopied] = useState(false)
  const resetTimerRef = useRef<number | null>(null)

  useEffect(() => {
    return () => {
      if (resetTimerRef.current !== null) {
        window.clearTimeout(resetTimerRef.current)
      }
    }
  }, [])

  const handleCopy = async () => {
    if (!content.trim()) return

    const success = await copyTextToClipboard(content)
    if (!success) return

    setCopied(true)

    if (resetTimerRef.current !== null) {
      window.clearTimeout(resetTimerRef.current)
    }

    resetTimerRef.current = window.setTimeout(() => {
      setCopied(false)
      resetTimerRef.current = null
    }, COPY_RESET_DELAY_MS)
  }

  return (
    <button
      type="button"
      onClick={() => {
        void handleCopy()
      }}
      data-slot="message-copy-button"
      aria-label={copied ? copiedLabel : copyLabel}
      title={copied ? copiedLabel : copyLabel}
      className={cn(
        "inline-flex items-center justify-center rounded-md border border-border/35 bg-background/88 p-1 text-muted-foreground shadow-none transition-colors hover:text-foreground focus-visible:text-foreground focus-visible:outline-none",
        className
      )}
    >
      {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
    </button>
  )
}

function AssistantTextBlock({
  content,
  streaming = false,
}: {
  content: string
  streaming?: boolean
}) {
  return (
    <div data-component="text-part" className="group/text-part">
      <div
        data-slot="text-part-body"
        className={`${MESSAGE_READING_MEASURE} group/text-part-body relative`}
      >
        <div
          data-slot="text-part-copy-wrapper"
          className="pointer-events-none absolute top-0 right-0 z-10 opacity-0 transition-opacity duration-150 group-focus-within/text-part-body:pointer-events-auto group-focus-within/text-part-body:opacity-100 group-hover/text-part-body:pointer-events-auto group-hover/text-part-body:opacity-100"
        >
          <MessageCopyButton
            content={content}
            copyLabel="Copy response"
            copiedLabel="Copied"
          />
        </div>
        <MarkdownContent
          content={content}
          streaming={streaming}
          className="text-body-sm leading-body-sm pr-10 text-pretty text-foreground/92"
        />
      </div>
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

  useEffect(() => {
    if (isStreaming) {
      setOpen(true)
    }
  }, [isStreaming])

  return (
    <div
      data-component="reasoning-part"
      className={`${MESSAGE_READING_MEASURE} py-1`}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="text-body-sm leading-body-sm flex w-full items-baseline gap-2 text-left"
      >
        {isStreaming ? (
          <span data-slot="tool-title">Thinking</span>
        ) : (
          <>
            <span data-slot="tool-title">Thought</span>
            {!open && lastLine && (
              <span
                data-slot="tool-subtitle"
                className="max-w-[400px] truncate"
              >
                {lastLine}
              </span>
            )}
          </>
        )}
      </button>
      {open && (
        <div className="text-body-sm leading-body-sm mt-2.5 border-l-2 border-border/30 pl-3">
          <MarkdownContent content={content} className="opacity-50" />
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
      return <AssistantTextBlock content={block.content} />
    case "failure":
      return (
        <div
          className={`${MESSAGE_READING_MEASURE} text-caption rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 font-medium text-destructive`}
        >
          {block.message}
        </div>
      )
    case "cancelled":
      return (
        <div
          className={`${MESSAGE_READING_MEASURE} text-caption rounded-lg border border-border/40 bg-muted/40 px-3 py-2 font-medium text-muted-foreground`}
        >
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
    <div
      className="text-heading-sm py-2 font-medium text-foreground"
      role="status"
      aria-live="polite"
      aria-atomic="true"
    >
      <Shimmer className="" as="span" duration={1} spread={3}>
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
    <div
      data-component="user-message"
      className="group/user-message flex w-full justify-start"
    >
      <div className="flex max-w-full flex-col items-start">
        <div
          data-slot="user-message-body"
          className="group/user-message-body relative w-full max-w-full rounded-md border border-border/45 bg-background/88 px-3 py-2.5"
        >
          <div
            data-slot="user-message-copy-wrapper"
            className="pointer-events-none absolute top-2 right-2 z-10 opacity-0 transition-opacity duration-150 group-focus-within/user-message-body:pointer-events-auto group-focus-within/user-message-body:opacity-100 group-hover/user-message-body:pointer-events-auto group-hover/user-message-body:opacity-100"
          >
            <MessageCopyButton
              content={content}
              copyLabel="Copy message"
              copiedLabel="Copied"
            />
          </div>
          <div
            data-slot="user-message-text"
            className="text-body-sm leading-body-sm max-w-full pr-10 text-pretty text-foreground/92"
          >
            <MarkdownContent content={content} />
          </div>
        </div>
      </div>
    </div>
  )
}

function TurnView({ turn }: { turn: TurnLifecycle }) {
  const grouped = groupBlocks(turn.blocks)

  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both] last:mb-0">
      <div className="mb-5">
        <UserMessageBlock content={turn.user_message} />
      </div>

      <div
        data-component="assistant-message"
        className="group/turn flex w-full flex-col gap-4"
      >
        {grouped.map((group, i) => {
          if (group.type === "tools") {
            return (
              <MemoizedToolGroup
                key={i}
                items={group.invocations.map(fromInvocation)}
                keepContextGroupsOpen
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
        <div className="mb-5">
          <UserMessageBlock content={streaming.userMessage} />
        </div>
      )}

      <div
        data-component="assistant-message"
        className="flex w-full flex-col gap-4"
      >
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
              <MemoizedStreamingToolGroup
                key={i}
                toolOutputs={group.tools}
                keepContextGroupsOpen
              />
            )
          }
          return (
            <AssistantTextBlock key={i} content={group.content} streaming />
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
