import { Loader2, ChevronUp } from "lucide-react"

export function ChatMessagesHistoryHint({
  historyLoadingMore,
  showHistoryHint,
}: {
  historyLoadingMore: boolean
  showHistoryHint: boolean
}) {
  return (
    <div
      className={
        showHistoryHint
          ? "pointer-events-none sticky top-0 z-10 -mx-4 mb-4 flex justify-center bg-gradient-to-b from-background via-background/95 to-transparent px-4 pt-3 pb-4 opacity-100 transition-opacity duration-200 sm:-mx-6 sm:px-6"
          : "pointer-events-none sticky top-0 z-10 -mx-4 mb-4 flex justify-center bg-gradient-to-b from-background via-background/95 to-transparent px-4 pt-3 pb-4 opacity-0 transition-opacity duration-200 sm:-mx-6 sm:px-6"
      }
      aria-hidden={!showHistoryHint}
    >
      <div
        className="inline-flex items-center gap-2 rounded-full border border-border/40 bg-muted/50 px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-sm backdrop-blur-md transition-colors duration-200"
        role={historyLoadingMore ? "status" : undefined}
        aria-live={historyLoadingMore ? "polite" : undefined}
        aria-atomic={historyLoadingMore ? "true" : undefined}
      >
        {historyLoadingMore ? (
          <>
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            <span>Loading history...</span>
          </>
        ) : (
          <>
            <ChevronUp className="h-3.5 w-3.5" />
            <span>Scroll up for older messages</span>
          </>
        )}
      </div>
    </div>
  )
}
