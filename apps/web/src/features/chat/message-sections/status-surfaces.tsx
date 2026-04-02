import { Shimmer } from "@/components/ai-elements/shimmer"
import type { StreamingTurn } from "@/lib/types"

const STATUS_LABELS: Record<StreamingTurn["status"], string> = {
  waiting: "Waiting",
  waiting_for_question: "Waiting for your answer",
  thinking: "Thinking",
  working: "Working",
  generating: "Generating",
  retrying: "Retrying",
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
      className="py-2"
      data-slot="tool-title"
      role="status"
      aria-live="polite"
      aria-atomic="true"
    >
      <Shimmer
        as="span"
        className="[--color-muted-foreground:var(--color-foreground)]"
      >
        {STATUS_LABELS[status]}
      </Shimmer>
    </div>
  )
}

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
    <div className="pointer-events-none sticky top-0 z-10 mb-4">
      <div
        className="mx-auto inline-flex items-center gap-2 rounded-full border border-border/40 bg-muted/50 px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-sm backdrop-blur-md"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        <span
          className={
            reducedMotion
              ? "size-2 rounded-full bg-primary/50"
              : "size-2 animate-pulse rounded-full bg-primary/50"
          }
        />
        <span>Loading session...</span>
      </div>
    </div>
  )
}
