import { ArrowDown } from "lucide-react"

export function ScrollToBottomButton({
  isAtBottom,
  onClick,
}: {
  isAtBottom: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-label="Scroll to bottom"
      className={
        isAtBottom
          ? "pointer-events-none absolute right-4 bottom-4 z-20 flex size-10 items-center justify-center rounded-full border border-border/50 bg-background/92 text-muted-foreground opacity-0 shadow-sm transition-opacity duration-200"
          : "absolute right-4 bottom-4 z-20 flex size-10 items-center justify-center rounded-full border border-border/50 bg-background/92 text-muted-foreground opacity-100 shadow-sm transition-opacity duration-200 hover:bg-muted hover:text-foreground"
      }
      tabIndex={isAtBottom ? -1 : 0}
      onClick={onClick}
    >
      <ArrowDown className="size-4" />
    </button>
  )
}
