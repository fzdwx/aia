import { describe, expect, test } from "vite-plus/test"

import {
  clampScrollTop,
  distanceFromBottom,
  shouldShowHistoryHint,
  shouldStickToBottom,
} from "./chat-messages-helpers"

describe("chat message scroll helpers", () => {
  test("clamps restored scroll positions to the available range", () => {
    expect(clampScrollTop(240, 180)).toBe(180)
    expect(clampScrollTop(-12, 180)).toBe(0)
    expect(clampScrollTop(120, 180)).toBe(120)
  })

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
