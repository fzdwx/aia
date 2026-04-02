import { ArrowDown } from "lucide-react"

export function ScrollToBottomButton({
  isAtBottom,
  onClick,
}: {
  isAtBottom: boolean
  onClick: () => void
}) {
  return (
    <div
      className={
        isAtBottom
          ? "pointer-events-none flex justify-center py-2 transition-opacity duration-200 opacity-0"
          : "flex justify-center py-2 transition-opacity duration-200 opacity-100"
      }
    >
      <button
        type="button"
        aria-label="Scroll to bottom"
        className="inline-flex items-center gap-1.5 rounded-full border border-border/50 bg-background/92 px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-sm backdrop-blur-sm transition-colors duration-200 hover:bg-muted hover:text-foreground"
        tabIndex={isAtBottom ? -1 : 0}
        onClick={onClick}
      >
        <ArrowDown className="size-3.5" />
      </button>
    </div>
  )
}
