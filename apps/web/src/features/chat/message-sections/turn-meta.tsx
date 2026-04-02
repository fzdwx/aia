import { RotateCcw } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { TurnLifecycle, TurnUsage } from "@/lib/types"

import { formatDurationMs } from "@/features/chat/tool-timeline-helpers"
import { useChatStore } from "@/stores/chat-store"

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

export function TurnMeta({
  turn,
  canRetry = false,
}: {
  turn: TurnLifecycle
  canRetry?: boolean
}) {
  const retryTurn = useChatStore((state) => state.retryTurn)
  const chatState = useChatStore((state) => state.chatState)
  const duration = formatDurationMs(turn.started_at_ms, turn.finished_at_ms)
  const statusLabel =
    turn.outcome === "cancelled"
      ? "cancelled"
      : turn.outcome === "waiting_for_question"
        ? "waiting for answer"
        : turn.outcome === "failed"
          ? "failed"
          : null

  if (!duration && !turn.usage && !statusLabel && !canRetry) return null

  return (
    <div className="text-caption mt-2 flex items-center gap-3 text-muted-foreground/55 opacity-0 transition-opacity duration-150 group-focus-within/turn:opacity-100 group-hover/turn:opacity-100">
      {duration ? (
        <span className="text-muted-foreground/65 tabular-nums">{`latency ${duration}`}</span>
      ) : null}
      {statusLabel ? (
        <span className="text-label rounded-full border border-border/40 px-2 py-0.5 font-medium tracking-[0.08em] text-muted-foreground/80">
          {statusLabel}
        </span>
      ) : null}
      {turn.usage ? (
        <span className="pointer-events-none">
          <TurnUsageBadge usage={turn.usage} />
        </span>
      ) : null}
      {canRetry ? (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Retry turn"
          title="Retry turn"
          disabled={chatState === "active"}
          onClick={() => retryTurn(turn.turn_id)}
        >
          <RotateCcw className="size-3.5" />
        </Button>
      ) : null}
    </div>
  )
}
