import { Children, isValidElement, type ReactNode } from "react"
import { readFileSync } from "node:fs"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import { ThemeProvider } from "@/components/theme-provider"

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

function renderWithTheme(content: ReactNode) {
  return renderToStaticMarkup(
    <ThemeProvider defaultTheme="dark">{content}</ThemeProvider>
  )
}

describe("tool renderer registry", () => {
  test("renders read tool title as file path only", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Read",
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

  test("renders read tool meta as a compact line range", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "Read",
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

    expect(badge.props.children).toBe("L121~160")
  })

  test("falls back to total lines for read meta instead of raw offset and limit", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "Read",
      arguments: {
        file_path: "apps/web/src/components/chat-messages.tsx",
        offset: 0,
        limit: 220,
      },
      details: {
        total_lines: 240,
      },
      outputContent: "",
      succeeded: true,
    })

    expect(meta).not.toBe(null)
    expect(isValidElement(meta)).toBe(true)
    if (!isValidElement<ElementWithChildren>(meta)) {
      throw new Error("expected read meta fallback badge to be a React element")
    }

    const badges = Children.toArray(meta.props.children)
    expect(badges).toHaveLength(1)
    const badge = badges[0]
    expect(isValidElement(badge)).toBe(true)
    if (!isValidElement<ElementWithChildren>(badge)) {
      throw new Error("expected read meta fallback badge to be a React element")
    }

    expect(badge.props.children).toBe("L1~240")
  })

  test("renders read tool details with expandable output for long content", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "Read",
      arguments: {
        file_path: "apps/web/src/components/chat-messages.tsx",
      },
      outputContent: Array.from(
        { length: 12 },
        (_, index) => `line ${index + 1}`
      ).join("\n"),
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("tool-timeline-output-toggle")
    expect(html).toContain("12 lines")
  })

  test("renders grep tool title as invocation args without repeating tool name", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Grep",
      arguments: {
        pattern: "renderMeta",
        path: "apps/web/src",
        glob: "*.ts",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe('"renderMeta" --glob "*.ts" apps/web/src')
  })

  test("renders grep tool meta from details", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "Grep",
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

  test("renders glob tool meta from details", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "Glob",
      arguments: {
        pattern: "src/**/*.ts",
        path: "apps/web/src",
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
    const html = renderToStaticMarkup(<>{meta}</>)
    expect(html).toContain("12 matches")
    expect(html).toContain("showing 5")
  })

  test("renders glob tool title like grep without repeating tool name", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Glob",
      arguments: {
        pattern: "src/**/*.ts",
        path: "apps/web/src",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe('"src/**/*.ts" apps/web/src')
  })

  test("renders codesearch tool title from query", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "CodeSearch",
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
      toolName: "CodeSearch",
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
      toolName: "CodeSearch",
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
    const html = renderWithTheme(details)
    expect(html).toContain("Top Result")
    expect(html).toContain("Code match")
    expect(html).toContain("useState Explained with Simple Examples")
    expect(html).toContain("https://react.wiki/hooks/use-state-explained/")
    expect(html).toContain("Practical examples and syntax for useState.")
  })

  test("renders websearch tool title and meta from query and options", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "WebSearch",
      arguments: {
        query: "AI news 2026",
        numResults: 5,
        type: "deep",
      },
      outputContent: "",
      succeeded: true,
    })
    const meta = toolRendererRegistry.renderMeta({
      toolName: "WebSearch",
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
      toolName: "WebSearch",
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
    const html = renderWithTheme(details)
    expect(html).toContain("Top Result")
    expect(html).toContain("Web result")
    expect(html).toContain("Latest AI News Roundup")
    expect(html).toContain("https://example.com/ai-news-2026")
    expect(html).toContain("example.com")
  })

  test("renders websearch preferred livecrawl badge", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "WebSearch",
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
      toolName: "Write",
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
      toolName: "Edit",
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
      toolName: "Edit",
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
      toolName: "Edit",
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

    const html = renderWithTheme(details)
    expect(html).toContain("<diffs-container")
    expect(html).toContain("tool-timeline-pierre-root")
    expect(html).not.toContain("Edited apps/web/src/index.css")
  })

  test("renders edit tool details from explicit diff before falling back to output text", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "Edit",
      arguments: {
        file_path: "apps/web/src/index.css",
        old_string: "old value",
        new_string: "new value",
      },
      details: {
        added: 1,
        removed: 1,
        diff: "@@\n-old value\n+newer value",
      },
      outputContent: "Edited apps/web/src/index.css",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("<diffs-container")
    expect(html).toContain("tool-timeline-pierre-root-patch")
    expect(html).not.toContain("Edited apps/web/src/index.css")
  })

  test("renders edit tool failure title from file path", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Edit",
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
      toolName: "Shell",
      arguments: {
        command: "cargo check -p agent-runtime",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("cargo check -p agent-runtime")
  })

  test("renders shell tool title from description when provided", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Shell",
      arguments: {
        command: "cargo check -p agent-runtime",
        description: "Run runtime crate checks",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("Run runtime crate checks")
  })

  test("renders shell tool details as command followed by output", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "Shell",
      arguments: {
        command: "cargo check -p agent-runtime",
        description: "Run runtime crate checks",
      },
      details: {
        command: "cargo check -p agent-runtime",
        stdout: "Finished dev [unoptimized] target(s)",
      },
      outputContent: "Finished dev [unoptimized] target(s)",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("tool-timeline-shell-detail")
    expect(html).toContain("tool-timeline-shell-body")
    expect(html).toContain("tool-timeline-shell-pre")
    expect(html).toContain(
      "$ cargo check -p agent-runtime\n\nFinished dev [unoptimized] target(s)"
    )
    expect(html).toContain("Finished dev [unoptimized] target(s)")
    expect(html).not.toContain("Result")
  })

  test("renders shell tool details with mixed stdout and stderr in one frame", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "Shell",
      arguments: {
        command: "cargo check -p agent-runtime",
      },
      details: {
        command: "cargo check -p agent-runtime",
        stdout: "warning summary",
        stderr: "warning: unused import",
      },
      outputContent: "warning summary\nwarning: unused import",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("$ cargo check -p agent-runtime\n\nwarning summary")
    expect(html).toContain("warning summary")
    expect(html).toContain("warning: unused import")
    expect(html).not.toContain("Result")
    expect(html).not.toContain("Failure")
  })

  test("renders apply_patch title from first patch operation", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "ApplyPatch",
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

    expect(title).toBe("apps/web/src/components/chat-messages.tsx")
  })

  test("renders write tool meta with green additions", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "Write",
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

  test("renders write tool details without a result section title", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "Write",
      arguments: {
        file_path: "apps/web/src/index.css",
        content: "first line\nsecond line",
      },
      details: {
        lines: 1,
      },
      outputContent: "Wrote 28 bytes to apps/web/src/index.css",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("<diffs-container")
    expect(html).toContain("tool-timeline-pierre-root")
    expect(html).not.toContain("Wrote 28 bytes to apps/web/src/index.css")
    expect(html).not.toContain("Result")
  })

  test("renders apply_patch meta with green additions and red removals", () => {
    const meta = toolRendererRegistry.renderMeta({
      toolName: "ApplyPatch",
      arguments: {
        patch:
          "*** Begin Patch\n*** Update File: apps/web/src/index.css\n@@\n-old\n+new\n*** End Patch",
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

  test("renders ApplyPatch details without a patch section title", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "ApplyPatch",
      arguments: {
        patch:
          "*** Begin Patch\n*** Update File: apps/web/src/index.css\n@@\n-old\n+new\n*** End Patch",
      },
      outputContent:
        "*** Begin Patch\n*** Update File: apps/web/src/index.css\n@@\n-old\n+new\n*** End Patch",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("tool-timeline-patch-list")
    expect(html).toContain("tool-timeline-patch-item")
    expect(html).toContain("apps/web/src/index.css")
    expect(html).toContain(">+1<")
    expect(html).toContain(">-1<")
    expect(html).toContain("<diffs-container")
    expect(html).not.toContain("tool-timeline-detail-title")
    expect(html).not.toContain('data-tool-detail-kind="patch"')
  })

  test("renders ApplyPatch add-file details with change counts", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "ApplyPatch",
      arguments: {
        patch:
          "*** Begin Patch\n*** Add File: .aia/note.txt\n+hello\n+world\n*** End Patch",
      },
      outputContent:
        "*** Begin Patch\n*** Add File: .aia/note.txt\n+hello\n+world\n*** End Patch",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain(".aia/")
    expect(html).toContain("note.txt")
    expect(html).toContain(">+2<")
    expect(html).toContain(">-0<")
    expect(html).toContain("tool-timeline-pierre-root-patch")
  })

  test("renders ApplyPatch move title with destination path", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "ApplyPatch",
      arguments: {
        patch:
          "*** Begin Patch\n*** Update File: old.txt\n*** Move to: nested/new.txt\n@@\n-old\n+new\n*** End Patch",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("old.txt → nested/new.txt")
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

  test("renders TapeHandoff title from name and summary", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "TapeHandoff",
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

  test("renders TapeInfo as inline summary metadata without details", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "TapeInfo",
      arguments: {},
      details: {
        total_entries: 12,
        anchor_count: 1,
        entries_since_last_anchor: 4,
        pressure_ratio: 0.7,
      },
      outputContent: '{"pressure_ratio":0.7}',
      succeeded: true,
    })
    const meta = toolRendererRegistry.renderMeta({
      toolName: "TapeInfo",
      arguments: {},
      details: {
        total_entries: 12,
        anchor_count: 1,
        entries_since_last_anchor: 4,
        pressure_ratio: 0.7,
      },
      outputContent: '{"pressure_ratio":0.7}',
      succeeded: true,
    })
    const details = toolRendererRegistry.renderDetails({
      toolName: "TapeInfo",
      arguments: {},
      details: {
        total_entries: 12,
        anchor_count: 1,
        entries_since_last_anchor: 4,
        pressure_ratio: 0.7,
      },
      outputContent: '{"pressure_ratio":0.7}',
      succeeded: true,
    })

    expect(title).toBe("pressure 70.0%")
    expect(meta).not.toBe(null)
    const html = renderToStaticMarkup(<>{meta}</>)
    expect(html).toContain("12 entries")
    expect(html).toContain("1 anchor")
    expect(html).toContain("+4 since anchor")
    expect(details).toBe(null)
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

    expect(title).toBe("src/main.rs · Recursive yes")
  })

  test("builds smarter fallback titles from arguments and details", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "custom.tool",
      arguments: {
        recursive: true,
      },
      details: {
        attempts: 2,
        exit_code: 1,
      },
      outputContent: "",
      succeeded: false,
    })

    expect(title).toBe("Recursive yes · Attempts 2")
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
    expect(source).toContain("@pierre/diffs/react")
    expect(source).toContain("MultiFileDiff")
    expect(source).toContain("PatchDiff")
    expect(source).toContain("useTheme")
    expect(source).toContain("pierre-dark")
    expect(source).toContain("pierre-light")
    expect(source).toContain("toolTimelineCopy.action.expand")
    expect(source).toContain("toolTimelineCopy.action.collapse")
    expect(source).toContain("toolTimelineCopy.section.content")
    expect(source).not.toContain('"Collapse"')
    expect(source).not.toContain('"Expand"')
    expect(source).not.toContain('"JSON"')
  })
})
