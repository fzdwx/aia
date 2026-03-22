import { Children, isValidElement, type ReactNode } from "react"
import { renderToStaticMarkup } from "react-dom/server"
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

  test("renders codesearch tool title from query", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.codesearch",
      arguments: {
        query: "React useState hook examples",
        tokensNum: 5000,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("React useState hook examples")
  })

  test("renders codesearch tool meta from tokens and result status", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.codesearch",
      arguments: {
        query: "Express.js middleware",
        tokensNum: 3000,
      },
      details: {
        result_found: false,
      },
      outputContent: "No code snippets or documentation found.",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    expect(isValidElement(meta)).toBe(true)
    if (!isValidElement<ElementWithChildren>(meta)) {
      throw new Error("expected codesearch meta to be a React element")
    }

    const badges = Children.toArray(meta.props.children)
    expect(badges).toHaveLength(2)

    const tokenBadge = badges[0]
    expect(isValidElement(tokenBadge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(tokenBadge)) {
      throw new Error("expected codesearch token badge to be a React element")
    }
    expect(tokenBadge.props.children).toBe("3,000 tok")

    const resultBadge = badges[1]
    expect(isValidElement(resultBadge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(resultBadge)) {
      throw new Error("expected codesearch result badge to be a React element")
    }
    expect(resultBadge.props.children).toBe("no result")
  })

  test("renders codesearch details with top result card", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "functions.codesearch",
      arguments: {
        query: "React useState hook examples",
        tokensNum: 5000,
      },
      outputContent: [
        "## useState Explained with Simple Examples | react.wiki",
        "https://react.wiki/hooks/use-state-explained/",
        "Practical examples and syntax for useState.",
        "const [count, setCount] = useState(0);",
      ].join("\n"),
      succeeded: true,
    })

    expect(details).not.toBe(null)
    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("Top Result")
    expect(html).toContain("Code match")
    expect(html).toContain("useState Explained with Simple Examples")
    expect(html).toContain("https://react.wiki/hooks/use-state-explained/")
    expect(html).toContain("Practical examples and syntax for useState.")
  })

  test("renders websearch tool title and meta from query and options", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.websearch",
      arguments: {
        query: "AI news 2026",
        numResults: 5,
        type: "deep",
      },
      outputContent: "",
      succeeded: true,
    })
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.websearch",
      arguments: {
        query: "AI news 2026",
        numResults: 5,
        type: "deep",
      },
      details: {
        result_found: true,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("AI news 2026")
    expect(meta).not.toBe(null)
    expect(isValidElement(meta)).toBe(true)
    if (!isValidElement<ElementWithChildren>(meta)) {
      throw new Error("expected websearch meta to be a React element")
    }

    const badges = Children.toArray(meta.props.children)
    expect(badges).toHaveLength(2)

    const resultBadge = badges[0]
    expect(isValidElement(resultBadge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(resultBadge)) {
      throw new Error("expected websearch result badge to be a React element")
    }
    expect(resultBadge.props.children).toBe("5 results")

    const typeBadge = badges[1]
    expect(isValidElement(typeBadge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(typeBadge)) {
      throw new Error("expected websearch type badge to be a React element")
    }
    expect(typeBadge.props.children).toBe("Deep")
  })

  test("renders websearch details with top result card", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "functions.websearch",
      arguments: {
        query: "AI news 2026",
        numResults: 8,
      },
      outputContent: [
        "## Latest AI News Roundup",
        "https://example.com/ai-news-2026",
        "Fresh updates from the AI industry in 2026.",
        "Funding, model launches, and policy shifts.",
      ].join("\n"),
      succeeded: true,
    })

    expect(details).not.toBe(null)
    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("Top Result")
    expect(html).toContain("Web result")
    expect(html).toContain("Latest AI News Roundup")
    expect(html).toContain("https://example.com/ai-news-2026")
    expect(html).toContain("example.com")
  })

  test("renders websearch preferred livecrawl badge", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.websearch",
      arguments: {
        query: "AI news 2026",
        numResults: 5,
        livecrawl: "preferred",
      },
      details: {
        result_found: true,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    expect(isValidElement(meta)).toBe(true)
    if (!isValidElement<ElementWithChildren>(meta)) {
      throw new Error("expected preferred livecrawl meta to be a React element")
    }

    const badges = Children.toArray(meta.props.children)
    expect(badges).toHaveLength(2)

    const livecrawlBadge = badges[1]
    expect(isValidElement(livecrawlBadge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(livecrawlBadge)) {
      throw new Error(
        "expected preferred livecrawl badge to be a React element"
      )
    }
    expect(livecrawlBadge.props.children).toBe("Live crawl")
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
