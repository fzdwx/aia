import { Children, isValidElement, type ReactNode } from "react"
import { describe, expect, test } from "vite-plus/test"

import { toolRendererRegistry } from "./index"

type ElementWithChildren = {
  children?: ReactNode
}

describe("tool renderer registry", () => {
  test("renders read tool title as file path only", () => {
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

    expect(title).toBe("apps/web/src/components/chat-messages.tsx")
  })

  test("renders read tool meta as requested range and actual line count", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.read",
      arguments: {
        file_path: "apps/web/src/components/chat-messages.tsx",
        offset: 120,
        limit: 40,
      },
      details: {
        lines_read: 40,
        total_lines: 120,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    expect(isValidElement(meta)).toBe(true)
    if (!isValidElement<ElementWithChildren>(meta)) {
      throw new Error("expected read meta to be a React element")
    }

    const badges = Children.toArray(meta.props.children)
    expect(badges).toHaveLength(1)
    const badge = badges[0]
    expect(isValidElement(badge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(badge)) {
      throw new Error("expected read meta badge to be a React element")
    }

    expect(badge.props.children).toBe("L121-160")
  })

  test("renders grep tool title as pattern only", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.grep",
      arguments: {
        pattern: "renderMeta",
        path: "apps/web/src",
        glob: "*.ts",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("renderMeta")
  })

  test("renders grep tool meta from details", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.grep",
      arguments: {
        pattern: "renderMeta",
      },
      details: {
        matches: 12,
        returned: 5,
        truncated: true,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
  })

  test("renders write tool title with compacted path", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.write",
      arguments: {
        file_path:
          "apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
        content: "new content",
      },
      details: {
        file_path:
          "apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
      },
      outputContent:
        "Wrote apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
      succeeded: true,
    })

    expect(title).toBe(
      ".../src/features/chat/tool-rendering/renderers/file-tools.tsx"
    )
  })

  test("renders edit tool title with compacted path", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.edit",
      arguments: {
        file_path:
          "apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
        old_string: "old value",
        new_string: "new value",
      },
      details: {
        file_path:
          "apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
      },
      outputContent:
        "Edited apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
      succeeded: true,
    })

    expect(title).toBe(
      ".../src/features/chat/tool-rendering/renderers/file-tools.tsx"
    )
  })

  test("renders edit tool failure title from file path", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.edit",
      arguments: {
        file_path: "apps/web/src/lib/tool-display.ts",
        old_string: "missing",
        new_string: "new value",
      },
      outputContent: "old_string not found in file",
      succeeded: false,
    })

    expect(title).toBe("apps/web/src/lib/tool-display.ts")
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

    expect(title).toBe(
      "session-cut — Condensed handoff summary for the next wake."
    )
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
