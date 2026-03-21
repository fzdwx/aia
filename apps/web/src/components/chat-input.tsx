import { useRef, useState } from "react"
import { ArrowUp, Square } from "lucide-react"

import { ModelSelector } from "@/components/model-selector"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { ThinkingLevel } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { useSessionSettingsStore } from "@/stores/session-settings-store"

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

const THINKING_OPTIONS: Array<{
  value: ThinkingLevel
  label: string
}> = [
  { value: "minimal", label: "Minimal" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
  { value: "xhigh", label: "XHigh" },
]

export function getThinkingLevelLabel(params: {
  reasoningValue: ThinkingLevel
  sessionSettingsHydrating: boolean
  sessionSettingsUpdating: boolean
}) {
  const { reasoningValue, sessionSettingsHydrating, sessionSettingsUpdating } =
    params

  if (sessionSettingsHydrating || sessionSettingsUpdating) {
    return "Thinking: Loading..."
  }

  return `Thinking: ${THINKING_OPTIONS.find((item) => item.value === reasoningValue)?.label ?? "Medium"}`
}

export function ChatInput() {
  const submitTurn = useChatStore((s) => s.submitTurn)
  const cancelTurn = useChatStore((s) => s.cancelTurn)
  const chatState = useChatStore((s) => s.chatState)
  const providerList = useChatStore((s) => s.providerList)
  const refreshProviders = useChatStore((s) => s.refreshProviders)
  const sessionSettings = useSessionSettingsStore((s) => s.sessionSettings)
  const sessionSettingsHydrating = useSessionSettingsStore((s) => s.hydrating)
  const sessionSettingsUpdating = useSessionSettingsStore((s) => s.updating)
  const sessionSettingsError = useSessionSettingsStore((s) => s.error)
  const supportsReasoning = useSessionSettingsStore((s) =>
    s.supportsReasoning(providerList)
  )
  const setReasoningEffort = useSessionSettingsStore((s) => s.setReasoningEffort)
  const switchModel = useSessionSettingsStore((s) => s.switchModel)
  const disabled = chatState === "active"

  const [value, setValue] = useState("")
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const canSend = value.trim().length > 0 && !disabled
  const reasoningValue = sessionSettings?.reasoning_effort ?? "medium"
  const settingsLoading = sessionSettingsHydrating || sessionSettingsUpdating
  const settingsBusy = settingsLoading || disabled

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
      <div className="pointer-events-none absolute -top-10 right-0 left-0 h-10 bg-gradient-to-t from-background to-transparent" />

      <div className="mx-auto max-w-[720px]">
        <div className="mb-1.5 flex items-center justify-start gap-2">
          <ModelSelector />
          {supportsReasoning && (
            <Select
              value={reasoningValue}
              disabled={settingsBusy}
              onValueChange={(next) => {
                if (!next) return
                void setReasoningEffort(providerList, next as ThinkingLevel)
                  .then((info) => {
                    if (info) refreshProviders()
                  })
                  .catch(() => {})
              }}
            >
              <SelectTrigger
                size="sm"
                className="h-7 border-0 bg-transparent px-1.5 py-0 text-[11px] text-muted-foreground shadow-none hover:bg-accent/50 hover:text-foreground/80 disabled:opacity-50"
              >
                <SelectValue>
                  {getThinkingLevelLabel({
                    reasoningValue,
                    sessionSettingsHydrating,
                    sessionSettingsUpdating,
                  })}
                </SelectValue>
              </SelectTrigger>
              <SelectContent align="start" alignItemWithTrigger={false}>
                {THINKING_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        </div>
        {sessionSettingsError && (
          <div className="mb-2 text-[11px] text-destructive/80">
            {sessionSettingsError}
          </div>
        )}
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
            onClick={disabled ? () => void cancelTurn() : handleSend}
            disabled={!disabled && !canSend}
            className={cn(
              "flex size-7 shrink-0 items-center justify-center rounded-lg transition-all duration-150",
              disabled
                ? "bg-amber-500/90 text-black hover:bg-amber-500"
                : canSend
                  ? "bg-foreground text-background hover:opacity-80"
                  : "bg-muted text-muted-foreground/30"
            )}
            title={disabled ? "Cancel current turn" : "Send message"}
          >
            {disabled ? (
              <Square className="size-3.5 fill-current" strokeWidth={2.5} />
            ) : (
              <ArrowUp className="size-4" strokeWidth={2.5} />
            )}
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
