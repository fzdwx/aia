import { describe, expect, test } from "vite-plus/test"

import {
  distanceFromBottom,
  shouldLoadOlderTurnsOnScroll,
  shouldShowHistoryHint,
  shouldStickToBottom,
  shouldTriggerOlderTurnsLoad,
} from "./helpers"

describe("chat message scroll helpers", () => {
  test("detects when the viewport is still near the bottom edge", () => {
    const nearBottom = distanceFromBottom({
      scrollHeight: 1200,
      scrollTop: 1038,
      clientHeight: 140,
    })
    const farFromBottom = distanceFromBottom({
      scrollHeight: 1200,
      scrollTop: 1020,
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

  test("triggers older history loading once scrolled into the top threshold", () => {
    expect(shouldTriggerOlderTurnsLoad(400)).toBe(true)
    expect(shouldTriggerOlderTurnsLoad(100)).toBe(true)
    expect(shouldTriggerOlderTurnsLoad(401)).toBe(false)
  })

  test("only pages older turns after an explicit upward scroll in an overflowing list", () => {
    expect(
      shouldLoadOlderTurnsOnScroll({
        scrollTop: 350,
        scrollHeight: 1200,
        clientHeight: 400,
        userScrolledUp: true,
      })
    ).toBe(true)
    expect(
      shouldLoadOlderTurnsOnScroll({
        scrollTop: 350,
        scrollHeight: 1200,
        clientHeight: 400,
        userScrolledUp: false,
      })
    ).toBe(false)
    expect(
      shouldLoadOlderTurnsOnScroll({
        scrollTop: 0,
        scrollHeight: 320,
        clientHeight: 400,
        userScrolledUp: true,
      })
    ).toBe(false)
  })
})
