import { describe, expect, test } from "vite-plus/test"

import { toolRendererRegistry } from "./index"

describe("tool renderer registry", () => {
  test("renders read tool title from arguments", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.read",
      arguments: {
        file_path: "apps/web/src/components/chat-messages.tsx",
        offset: 120,
        limit: 40,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe(
      "apps/web/src/components/chat-messages.tsx — from 120 · limit 40"
    )
  })

  test("renders shell tool title from command", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.shell",
      arguments: {
        command: "cargo check -p agent-runtime",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("cargo check -p agent-runtime")
  })

  test("renders apply_patch title from first patch operation", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.apply_patch",
      arguments: {
        patch: [
          "*** Begin Patch",
          "*** Update File: apps/web/src/components/chat-messages.tsx",
          "@@",
          "-old",
          "+new",
          "*** End Patch",
        ].join("\n"),
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe(
      "*** Update File: apps/web/src/components/chat-messages.tsx"
    )
  })

  test("renders tape_handoff title from name and summary", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "tape_handoff",
      arguments: {
        name: "session-cut",
        summary: "Condensed handoff summary for the next wake.",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("session-cut — Condensed handoff summary for the next wake.")
  })

  test("falls back to default renderer for unknown tools", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "custom.tool",
      arguments: {
        path: "src/main.rs",
        recursive: true,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("src/main.rs — path: src/main.rs · recursive: true")
  })
})
