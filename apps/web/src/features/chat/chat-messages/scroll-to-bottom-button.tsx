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
          ? "pointer-events-none inline-flex h-9 items-center justify-center rounded-full border border-border/50 bg-background/92 px-3 text-sm text-muted-foreground opacity-0 shadow-sm transition-all duration-200"
          : "inline-flex h-9 items-center justify-center rounded-full border border-border/50 bg-background/92 px-3 text-sm text-muted-foreground opacity-100 shadow-sm transition-all duration-200 hover:bg-muted hover:text-foreground"
      }
      tabIndex={isAtBottom ? -1 : 0}
      onClick={onClick}
    >
      <ArrowDown className="size-4" />
    </button>
  )
}
