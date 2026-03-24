import { afterEach, describe, expect, test, vi } from "vite-plus/test"

import { copyTextToClipboard } from "./clipboard"

describe("copyTextToClipboard", () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  test("returns false when clipboard is unavailable", async () => {
    vi.stubGlobal("navigator", {})

    await expect(copyTextToClipboard("hello")).resolves.toBe(false)
  })

  test("writes to clipboard when available", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)

    vi.stubGlobal("navigator", {
      clipboard: { writeText },
    })

    await expect(copyTextToClipboard("hello")).resolves.toBe(true)
    expect(writeText).toHaveBeenCalledWith("hello")
  })

  test("returns false when clipboard write fails", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"))

    vi.stubGlobal("navigator", {
      clipboard: { writeText },
    })

    await expect(copyTextToClipboard("hello")).resolves.toBe(false)
  })
})
