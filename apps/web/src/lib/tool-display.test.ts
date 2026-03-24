import { describe, expect, test } from "vite-plus/test"
import assert from "node:assert/strict"

import { getToolDisplayName, getToolDisplayPath } from "./tool-display"

describe("tool display name", () => {
  test("uses real backend tool names directly", () => {
    expect(getToolDisplayName("Read")).toBe("Read")
  })

  test("preserves PascalCase backend tool names for display", () => {
    expect(getToolDisplayName("ApplyPatch")).toBe("ApplyPatch")
    expect(getToolDisplayName("TapeInfo")).toBe("TapeInfo")
  })
})

describe("tool display path", () => {
  test("uses namespaced read tool file_path argument", () => {
    assert.equal(
      getToolDisplayPath("Read", undefined, {
        file_path: "/home/like/projects/aia/AGENTS.md",
      }),
      "/home/like/projects/aia/AGENTS.md"
    )
  })

  test("prefers command for namespaced shell tool", () => {
    assert.equal(
      getToolDisplayPath("Shell", undefined, {
        command: "pwd",
      }),
      "pwd"
    )
  })

  test("uses query for namespaced codesearch tool", () => {
    assert.equal(
      getToolDisplayPath("CodeSearch", undefined, {
        query: "React useState hook examples",
        tokensNum: 5000,
      }),
      "React useState hook examples"
    )
  })

  test("uses query for namespaced websearch tool", () => {
    assert.equal(
      getToolDisplayPath("WebSearch", undefined, {
        query: "AI news 2026",
        numResults: 8,
      }),
      "AI news 2026"
    )
  })

  test("uses query for PascalCase code search tool", () => {
    assert.equal(
      getToolDisplayPath("CodeSearch", undefined, {
        query: "React useState hook examples",
        tokensNum: 5000,
      }),
      "React useState hook examples"
    )
  })
})
