import { describe, expect, test } from "vite-plus/test"

import {
  distanceFromBottom,
  shouldLoadOlderTurnsOnScroll,
  shouldResumeAutoFollow,
  shouldShowHistoryHint,
  shouldStickToBottom,
  shouldTriggerOlderTurnsLoad,
} from "./helpers"

describe("chat message scroll helpers", () => {
  test("detects when the viewport is still near the bottom edge", () => {
    const nearBottom = distanceFromBottom({
      scrollHeight: 1200,
      scrollTop: 1028,
      clientHeight: 140,
    })
    const farFromBottom = distanceFromBottom({
      scrollHeight: 1200,
      scrollTop: 1000,
      clientHeight: 140,
    })

    expect(shouldStickToBottom(nearBottom)).toBe(true)
    expect(shouldStickToBottom(farFromBottom)).toBe(false)
  })

  test("resumes auto follow with a slightly larger bottom threshold", () => {
    expect(shouldResumeAutoFollow(72)).toBe(true)
    expect(shouldResumeAutoFollow(120)).toBe(false)
  })

  test("shows the history hint while loading or when the user is near the top", () => {
    expect(shouldShowHistoryHint(true, 400)).toBe(true)
    expect(shouldShowHistoryHint(false, 120)).toBe(true)
    expect(shouldShowHistoryHint(false, 260)).toBe(false)
  })

  test("triggers older history loading based on viewport height ratio", () => {
    const clientHeight = 400
    // 1.5 * 400 = 600
    expect(shouldTriggerOlderTurnsLoad(600, clientHeight)).toBe(true)
    expect(shouldTriggerOlderTurnsLoad(400, clientHeight)).toBe(true)
    expect(shouldTriggerOlderTurnsLoad(601, clientHeight)).toBe(false)
  })

  test("only pages older turns after an explicit upward scroll in an overflowing list", () => {
    const clientHeight = 400
    // Trigger threshold is 1.5 * 400 = 600
    expect(
      shouldLoadOlderTurnsOnScroll({
        scrollTop: 500,
        scrollHeight: 1200,
        clientHeight,
        userScrolledUp: true,
      })
    ).toBe(true)
    expect(
      shouldLoadOlderTurnsOnScroll({
        scrollTop: 500,
        scrollHeight: 1200,
        clientHeight,
        userScrolledUp: false,
      })
    ).toBe(false)
    expect(
      shouldLoadOlderTurnsOnScroll({
        scrollTop: 0,
        scrollHeight: 320,
        clientHeight,
        userScrolledUp: true,
      })
    ).toBe(false)
  })
})
