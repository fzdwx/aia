import { useRef, useState } from "react"
import { ArrowUp } from "lucide-react"
import { cn } from "@/lib/utils"
import { ModelSelector } from "@/components/model-selector"
import { useChatStore } from "@/stores/chat-store"

function ContextPressure() {
  const pressure = useChatStore((s) => s.contextPressure)
  if (pressure == null || pressure <= 0.6) return <span />

  const pct = Math.round(pressure * 100)
  const color =
    pressure > 0.95
      ? "text-destructive/70"
      : pressure > 0.8
        ? "text-amber-500/70"
        : "text-muted-foreground/50"

  return <span className={cn("tabular-nums", color)}>{pct}%</span>
}

export function ChatInput() {
  const submitTurn = useChatStore((s) => s.submitTurn)
  const chatState = useChatStore((s) => s.chatState)
  const disabled = chatState === "active"

  const [value, setValue] = useState("")
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const canSend = value.trim().length > 0 && !disabled

  function handleSend() {
    if (!canSend) return
    submitTurn(value.trim())
    setValue("")
    textareaRef.current?.focus()
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  return (
    <div className="relative shrink-0 border-t border-border/30 px-4 pt-3 pb-4">
      {/* Fade gradient above input */}
      <div className="pointer-events-none absolute -top-10 right-0 left-0 h-10 bg-gradient-to-t from-background to-transparent" />

      <div className="mx-auto max-w-[720px]">
        <ModelSelector />
        <div className="flex items-end gap-3 rounded-xl border border-border/50 bg-card px-4 py-3">
          <textarea
            ref={textareaRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Message aia..."
            rows={1}
            className="max-h-[160px] min-h-[24px] flex-1 resize-none bg-transparent text-[14px] leading-relaxed text-foreground outline-none placeholder:text-muted-foreground/40"
            style={{ fieldSizing: "content" } as React.CSSProperties}
          />
          <button
            onClick={handleSend}
            disabled={!canSend}
            className={cn(
              "flex size-7 shrink-0 items-center justify-center rounded-lg transition-all duration-150",
              canSend
                ? "bg-foreground text-background hover:opacity-80"
                : "bg-muted text-muted-foreground/30"
            )}
          >
            <ArrowUp className="size-4" strokeWidth={2.5} />
          </button>
        </div>
        <div className="mt-2 flex items-center justify-between text-[11px]">
          <ContextPressure />
          <p className="text-muted-foreground/30">
            aia may produce inaccurate responses.
          </p>
        </div>
      </div>
    </div>
  )
}
