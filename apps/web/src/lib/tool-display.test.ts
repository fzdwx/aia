import { describe, expect, test } from "vite-plus/test"
import assert from "node:assert/strict"

import { getToolDisplayName, getToolDisplayPath } from "./tool-display"

describe("tool display name", () => {
  test("normalizes namespaced tool names", () => {
    expect(getToolDisplayName("functions.read")).toBe("read")
  })
})

describe("tool display path", () => {
  test("uses namespaced read tool file_path argument", () => {
    assert.equal(
      getToolDisplayPath("functions.read", undefined, {
        file_path: "/home/like/projects/aia/AGENTS.md",
      }),
      "/home/like/projects/aia/AGENTS.md"
    )
  })

  test("prefers command for namespaced shell tool", () => {
    assert.equal(
      getToolDisplayPath("functions.shell", undefined, {
        command: "pwd",
      }),
      "pwd"
    )
  })
})
