import { describe, expect, test } from "vite-plus/test"

import {
  distanceFromBottom,
  shouldShowHistoryHint,
  shouldStickToBottom,
} from "./chat-messages-helpers"

describe("chat message scroll helpers", () => {
  test("detects when the viewport is still near the bottom edge", () => {
    const nearBottom = distanceFromBottom({
      scrollHeight: 1200,
      scrollTop: 980,
      clientHeight: 140,
    })
    const farFromBottom = distanceFromBottom({
      scrollHeight: 1200,
      scrollTop: 760,
      clientHeight: 140,
    })

    expect(shouldStickToBottom(nearBottom)).toBe(true)
    expect(shouldStickToBottom(farFromBottom)).toBe(false)
  })

  test("shows the history hint while loading or when the user is near the top", () => {
    expect(shouldShowHistoryHint(true, 400)).toBe(true)
    expect(shouldShowHistoryHint(false, 120)).toBe(true)
    expect(shouldShowHistoryHint(false, 260)).toBe(false)
  })
})
