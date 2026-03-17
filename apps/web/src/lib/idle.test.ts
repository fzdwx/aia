import { describe, test } from "node:test"
import assert from "node:assert/strict"

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

    assert.equal(scheduled, true)
    assert.equal(handle, 7)
    assert.equal(cancelledHandle, 7)
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

    assert.equal(scheduledDelay, 120)
    assert.equal(handle, 13)
    assert.equal(clearedHandle, 13)
  })
})
