import type { ThinkingLevel } from "@/lib/types"

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

export { THINKING_OPTIONS }
