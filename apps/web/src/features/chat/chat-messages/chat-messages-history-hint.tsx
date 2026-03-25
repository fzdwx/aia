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
          ? "pointer-events-none sticky top-0 z-10 -mx-4 mb-4 flex justify-center bg-gradient-to-b from-background via-background/94 to-transparent px-4 pt-2 pb-3 opacity-100 transition-opacity duration-150 sm:-mx-6 sm:px-6"
          : "pointer-events-none sticky top-0 z-10 -mx-4 mb-4 flex justify-center bg-gradient-to-b from-background via-background/94 to-transparent px-4 pt-2 pb-3 opacity-0 transition-opacity duration-150 sm:-mx-6 sm:px-6"
      }
      aria-hidden={!showHistoryHint}
    >
      <div
        className="max-w-full rounded-full border border-border/35 bg-background/88 px-3 py-1 text-center text-xs text-muted-foreground/80 shadow-none"
        role={historyLoadingMore ? "status" : undefined}
        aria-live={historyLoadingMore ? "polite" : undefined}
        aria-atomic={historyLoadingMore ? "true" : undefined}
      >
        {historyLoadingMore
          ? "Loading older messages…"
          : "Scroll up for older messages"}
      </div>
    </div>
  )
}
