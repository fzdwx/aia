import { useRef, useState } from "react"
import { ArrowUp, Square, X, ListOrdered } from "lucide-react"

import { ModelSelector } from "./model-selector"
import { PendingQuestionComposer } from "./pending-question-composer"
import { ReasoningSelector } from "./reasoning-selector"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { usePendingQuestionStore } from "@/stores/pending-question-store"
import { useSessionSettingsStore } from "@/stores/session-settings-store"

const CHAT_MICRO_TEXT = "text-meta"
const CHAT_INPUT_HELP = "text-muted-foreground/30"

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

function MessageQueue() {
  const messageQueue = useChatStore((s) => s.messageQueue)
  const deleteQueuedMessage = useChatStore((s) => s.deleteQueuedMessage)
  const chatState = useChatStore((s) => s.chatState)

  if (messageQueue.length === 0) return null

  return (
    <div className="mb-2 rounded-lg border border-border/50 bg-muted/30 px-3 py-2">
      <div className="mb-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
        <ListOrdered className="size-3" />
        <span>Queued messages ({messageQueue.length})</span>
      </div>
      <div className="space-y-1">
        {messageQueue.map((msg, index) => (
          <div
            key={msg.id}
            className="flex items-center gap-2 rounded bg-background/50 px-2 py-1 text-xs"
          >
            <span className="text-muted-foreground">{index + 1}.</span>
            <span className="flex-1 truncate">{msg.content}</span>
            {chatState === "idle" && (
              <button
                onClick={() => void deleteQueuedMessage(msg.id)}
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="size-3" />
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

export function ChatInput() {
  const sendMessage = useChatStore((s) => s.sendMessage)
  const interruptTurn = useChatStore((s) => s.interruptTurn)
  const chatState = useChatStore((s) => s.chatState)
  const pendingQuestion = usePendingQuestionStore((s) => s.pendingQuestion)
  const sessionSettingsError = useSessionSettingsStore((s) => s.error)
  const [value, setValue] = useState("")
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const isActive = chatState === "active"

  if (pendingQuestion) {
    return <PendingQuestionComposer />
  }

  const canSend = value.trim().length > 0

  function handleSend() {
    if (!canSend) return
    sendMessage(value.trim())
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
      <div className="pointer-events-none absolute -top-10 right-0 left-0 h-10 bg-gradient-to-t from-background to-transparent" />

      <div className="mx-auto max-w-[720px]">
        <div className="mb-1.5 flex items-center justify-start gap-2">
          <ModelSelector />
          <ReasoningSelector />
        </div>
        {sessionSettingsError && (
          <div className={`mb-2 ${CHAT_MICRO_TEXT} text-destructive/80`}>
            {sessionSettingsError}
          </div>
        )}
        <MessageQueue />
        <div className="flex items-end gap-3 rounded-xl border border-border/50 bg-card px-4 py-3">
          <textarea
            ref={textareaRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Message aia..."
            rows={1}
            className="text-body-sm leading-body-sm max-h-[160px] min-h-[24px] flex-1 resize-none bg-transparent text-foreground outline-none placeholder:text-muted-foreground/40"
            style={{ fieldSizing: "content" } as React.CSSProperties}
          />
          <button
            onClick={isActive ? () => void interruptTurn() : handleSend}
            disabled={!isActive && !canSend}
            className={cn(
              "flex size-7 shrink-0 items-center justify-center rounded-lg transition-all duration-150",
              isActive
                ? "bg-amber-500/90 text-black hover:bg-amber-500"
                : canSend
                  ? "bg-foreground text-background hover:opacity-80"
                  : "bg-muted text-muted-foreground/30"
            )}
            title={isActive ? "Interrupt (ESC)" : "Send message"}
          >
            {isActive ? (
              <Square className="size-3.5 fill-current" strokeWidth={2.5} />
            ) : (
              <ArrowUp className="size-4" strokeWidth={2.5} />
            )}
          </button>
        </div>
        <div
          className={`mt-2 flex items-center justify-between ${CHAT_MICRO_TEXT}`}
        >
          <ContextPressure />
          <p className={CHAT_INPUT_HELP}>
            {isActive
              ? "Press button or ESC to interrupt"
              : "aia may produce inaccurate responses."}
          </p>
        </div>
      </div>
    </div>
  )
}
