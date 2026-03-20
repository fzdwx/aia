import { describe, expect, test } from "vite-plus/test"

import {
  asArray,
  asRecord,
  asString,
  extractTraceText,
} from "@/lib/trace-inspection"

describe("trace inspection", () => {
  test("extracts nested text payloads", () => {
    expect(
      extractTraceText({
        content: [{ text: "hello" }, { value: "world" }],
      })
    ).toBe("hello\nworld")
  })

  test("normalizes common JSON payload helpers", () => {
    expect(asRecord({ ok: true })).toEqual({ ok: true })
    expect(asRecord("bad")).toBeNull()
    expect(asString("hello")).toBe("hello")
    expect(asString("")).toBeNull()
    expect(asArray([1, 2, 3])).toEqual([1, 2, 3])
    expect(asArray(null)).toEqual([])
  })
})
