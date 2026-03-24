import { Children, isValidElement, type ReactNode } from "react"
import { readFileSync } from "node:fs"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import { toolRendererRegistry } from "./index"

function loadToolRenderingUiSource() {
  return readFileSync(new URL("./ui.tsx", import.meta.url), "utf8").replace(
    /\s+/g,
    " "
  )
}

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

  test("renders edit tool meta with green additions and red removals", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.edit",
      arguments: {
        file_path:
          "apps/web/src/features/chat/tool-rendering/renderers/file-tools.tsx",
      },
      details: {
        added: 5,
        removed: 14,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    const html = renderToStaticMarkup(<>{meta}</>)
    expect(html).toContain("+5")
    expect(html).toContain("text-emerald-400")
    expect(html).toContain("-14")
    expect(html).toContain("text-red-400")
  })

  test("renders edit tool details from old and new strings when diff is missing", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "functions.edit",
      arguments: {
        file_path: "apps/web/src/index.css",
        old_string: "old value\nline 2",
        new_string: "new value\nline 2\nline 3",
      },
      details: {
        added: 3,
        removed: 2,
      },
      outputContent: "Edited apps/web/src/index.css",
      succeeded: true,
    })

    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("-old value")
    expect(html).toContain("-line 2")
    expect(html).toContain("+new value")
    expect(html).toContain("+line 3")
    expect(html).not.toContain("Edited apps/web/src/index.css")
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

    expect(title).toBe("Shell — cargo check -p agent-runtime")
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

    expect(title).toBe("Patch — apps/web/src/components/chat-messages.tsx")
  })

  test("renders write tool meta with green additions", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.write",
      arguments: {
        file_path: "apps/web/src/index.css",
      },
      details: {
        lines: 12,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    const html = renderToStaticMarkup(<>{meta}</>)
    expect(html).toContain("+12")
    expect(html).toContain("text-emerald-400")
  })

  test("renders apply_patch meta with green additions and red removals", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "functions.apply_patch",
      arguments: {
        patch: "*** Begin Patch\n*** End Patch",
      },
      details: {
        lines_added: 4,
        lines_removed: 2,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    const html = renderToStaticMarkup(<>{meta}</>)
    expect(html).toContain("+4")
    expect(html).toContain("-2")
  })

  test("renders question tool summary and ignored semantics", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "functions.question",
      arguments: {
        question: "Override existing config?",
      },
      details: {
        status: "ignored",
      },
      outputContent: "ignored by user",
      succeeded: true,
    })

    const details = toolRendererRegistry.renderDetails({
      toolName: "functions.question",
      arguments: {
        question: "Override existing config?",
      },
      details: {
        status: "ignored",
      },
      outputContent: "ignored by user",
      succeeded: true,
    })

    expect(title).toBe("Override existing config?")
    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("Issue Ignored")
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

    expect(title).toBe("Unknown · custom.tool — src/main.rs")
  })

  test("renders unknown tool details with chinese fallback sections", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "custom.tool",
      arguments: {
        path: "src/main.rs",
        recursive: true,
      },
      details: {
        attempts: 2,
      },
      outputContent: "command failed with exit code 1",
      succeeded: false,
    })

    expect(details).not.toBe(null)
    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("Input")
    expect(html).toContain("Raw Details")
    expect(html).toContain("Failure")
    expect(html).toContain("command failed with exit code 1")
  })

  test("reads shared timeline copy for expand and section labels", () => {
    const source = loadToolRenderingUiSource()

    expect(source).toContain(
      'import { toolTimelineCopy } from "../tool-timeline-copy"'
    )
    expect(source).toContain("toolTimelineCopy.action.expand")
    expect(source).toContain("toolTimelineCopy.action.collapse")
    expect(source).toContain("toolTimelineCopy.section.content")
    expect(source).not.toContain('"Collapse"')
    expect(source).not.toContain('"Expand"')
    expect(source).not.toContain('"JSON"')
  })
})
