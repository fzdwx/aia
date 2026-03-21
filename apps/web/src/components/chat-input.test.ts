import { describe, expect, test } from "vite-plus/test"

import { getThinkingLevelLabel } from "./chat-input"

describe("getThinkingLevelLabel", () => {
  test("keeps current label while chat is active but settings are already loaded", () => {
    expect(
      getThinkingLevelLabel({
        reasoningValue: "high",
        sessionSettingsHydrating: false,
        sessionSettingsUpdating: false,
      })
    ).toBe("Thinking: High")
  })

  test("shows loading only while session settings are hydrating", () => {
    expect(
      getThinkingLevelLabel({
        reasoningValue: "high",
        sessionSettingsHydrating: true,
        sessionSettingsUpdating: false,
      })
    ).toBe("Thinking: Loading...")
  })

  test("shows loading while updating reasoning effort", () => {
    expect(
      getThinkingLevelLabel({
        reasoningValue: "low",
        sessionSettingsHydrating: false,
        sessionSettingsUpdating: true,
      })
    ).toBe("Thinking: Loading...")
  })
})
