import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { getThinkingLevelLabel, THINKING_OPTIONS } from "./thinking"
import type { ThinkingLevel } from "@/lib/types"
import { useChatStore } from "@/stores/chat-store"
import { useSessionSettingsStore } from "@/stores/session-settings-store"

const REASONING_SELECTOR_ITEM = "text-ui px-2.5 py-1.5"

export function ReasoningSelector() {
  const providerList = useChatStore((s) => s.providerList)
  const chatState = useChatStore((s) => s.chatState)
  const setSessionReasoningEffort = useChatStore(
    (s) => s.setSessionReasoningEffort
  )
  const sessionSettings = useSessionSettingsStore((s) => s.sessionSettings)
  const hydrating = useSessionSettingsStore((s) => s.hydrating)
  const updating = useSessionSettingsStore((s) => s.updating)
  const supportsReasoning = useSessionSettingsStore((s) =>
    s.supportsReasoning(providerList)
  )

  if (!supportsReasoning) return null

  const reasoningValue = sessionSettings?.reasoning_effort ?? "medium"

  return (
    <div className="min-w-0">
      <Select
        value={reasoningValue}
        disabled={hydrating || updating || chatState === "active"}
        onValueChange={(next) => {
          if (!next) return
          void setSessionReasoningEffort(next as ThinkingLevel).catch(() => {})
        }}
      >
        <SelectTrigger
          size="sm"
          className="text-ui h-8 max-w-[240px] border-0 bg-transparent px-2 py-1 font-medium text-muted-foreground/90 shadow-none hover:bg-muted/60 hover:text-foreground/80 disabled:opacity-50 [&_svg:not([class*='size-'])]:size-3"
        >
          <SelectValue className="text-ui">
            {getThinkingLevelLabel({
              reasoningValue,
              sessionSettingsHydrating: hydrating,
              sessionSettingsUpdating: updating,
            })}
          </SelectValue>
        </SelectTrigger>
        <SelectContent
          align="start"
          alignItemWithTrigger={false}
          className="min-w-[220px]"
        >
          {THINKING_OPTIONS.map((option) => (
            <SelectItem
              key={option.value}
              value={option.value}
              className={REASONING_SELECTOR_ITEM}
            >
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
