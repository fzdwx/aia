import { describe, expect, test } from "vite-plus/test"

import { createIdleScheduler } from "./idle"

describe("idle scheduler", () => {
  test("uses requestIdleCallback when available", () => {
    let scheduled = false
    let cancelledHandle: number | null = null

    const target = {
      requestIdleCallback: (callback: () => void) => {
        scheduled = true
        callback()
        return 7
      },
      cancelIdleCallback: (handle: number) => {
        cancelledHandle = handle
      },
    } as unknown as Window

    const { schedule, cancel } = createIdleScheduler(target)
    const handle = schedule(() => {})
    cancel(handle)

    expect(scheduled).toBe(true)
    expect(handle).toBe(7)
    expect(cancelledHandle).toBe(7)
  })

  test("falls back to setTimeout when requestIdleCallback is unavailable", () => {
    let scheduledDelay: number | null = null
    let clearedHandle: number | null = null

    const target = {
      setTimeout: (callback: () => void, delay?: number) => {
        scheduledDelay = delay ?? null
        callback()
        return 13
      },
      clearTimeout: (handle: number) => {
        clearedHandle = handle
      },
    } as unknown as Window

    const { schedule, cancel } = createIdleScheduler(target)
    const handle = schedule(() => {})
    cancel(handle)

    expect(scheduledDelay).toBe(120)
    expect(handle).toBe(13)
    expect(clearedHandle).toBe(13)
  })
})
