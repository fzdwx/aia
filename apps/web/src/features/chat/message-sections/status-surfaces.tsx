import { Shimmer } from "@/components/ai-elements/shimmer"
import type { StreamingTurn } from "@/lib/types"

const STATUS_LABELS: Record<StreamingTurn["status"], string> = {
  waiting: "Waiting",
  waiting_for_question: "Waiting for your answer",
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
      className="py-2"
      data-slot="tool-title"
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
