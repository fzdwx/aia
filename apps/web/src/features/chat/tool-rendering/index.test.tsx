import { Children, isValidElement, type ReactNode } from "react"
import { readFileSync } from "node:fs"
import { renderToStaticMarkup } from "react-dom/server"
import { beforeEach, describe, expect, test } from "vite-plus/test"

import { ThemeProvider } from "@/components/theme-provider"

import { toolRendererRegistry } from "./index"
import { setActiveWorkspaceRoot } from "@/lib/tool-display"

function loadToolRenderingUiSource() {
  return readFileSync(new URL("./ui.tsx", import.meta.url), "utf8").replace(
    /\s+/g,
    " "
  )
}

function loadPierreDiffSource() {
  return readFileSync(
    new URL("../diff/pierre-diff.tsx", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadPierreDiffProviderSource() {
  return readFileSync(
    new URL("../diff/pierre-diff-provider.tsx", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadPierreConfigSource() {
  return readFileSync(
    new URL("../diff/pierre-config.ts", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadAppIndexCssSource() {
  return readFileSync(
    new URL("../../../index.css", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
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
  beforeEach(() => {
    setActiveWorkspaceRoot("/home/like/projects/like")
  })

  test("renders read tool title as filename plus parent path", () => {
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

    expect(title).toBe("chat-messages.tsx apps/web/src/components")
  })

  test("renders current-directory file tool title as filename only", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Read",
      arguments: {
        file_path: "/home/like/projects/like/tool-display.ts",
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("tool-display.ts")
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

  test("renders shell subtitle from description instead of first output line", () => {
    const subtitle = toolRendererRegistry.renderSubtitle({
      toolName: "Shell",
      arguments: {
        command: "pnpm run test",
        description: "running 32 tests",
      },
      details: {
        command: "aia@0.0.1 test /home/like/projects/aia/apps/web",
      },
      outputContent: "aia@0.0.1 test /home/like/projects/aia/apps/web\nPASS",
      succeeded: true,
    })

    expect(subtitle).toBe("running 32 tests")
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

  test("renders write tool title as filename plus compacted parent path", () => {
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
      "file-tools.tsx .../src/features/chat/tool-rendering/renderers"
    )
  })

  test("renders edit tool title as filename plus compacted parent path", () => {
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
      "file-tools.tsx .../src/features/chat/tool-rendering/renderers"
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

  test("renders edit tool details from old and new strings instead of explicit diff text", () => {
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
    expect(html).toContain("tool-timeline-pierre-root-multi")
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

    expect(title).toBe("tool-display.ts apps/web/src/lib")
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

  test("renders shell subtitle from first output line when arguments are missing", () => {
    const subtitle = toolRendererRegistry.renderSubtitle({
      toolName: "Shell",
      arguments: {},
      outputContent: "npm run check\nsecond line",
      succeeded: true,
    })

    expect(subtitle).toBe("npm run check")
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
    expect(html).toContain("tool-timeline-shell-command")
    expect(html).toContain("$ cargo check -p agent-runtime")
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
    expect(html).toContain("tool-timeline-shell-command")
    expect(html).toContain("$ cargo check -p agent-runtime")
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

  test("renders apply_patch subtitle from first patch operation", () => {
    const subtitle = toolRendererRegistry.renderSubtitle({
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

    expect(subtitle).toBe("apps/web/src/components/chat-messages.tsx")
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
    expect(html).toContain("text-emerald-400")
    expect(html).toContain("-2")
    expect(html).toContain("text-red-400")
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
    expect(html).toContain("index.css")
    expect(html).toContain("apps/web/src/")
    expect(html).toContain(">+1<")
    expect(html).toContain(">-1<")
    expect(html).toContain("text-emerald-400")
    expect(html).toContain("text-red-400")
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
    expect(html).toContain("note.txt")
    expect(html).toContain(".aia/")
    expect(html).toContain(">+2<")
    expect(html).toContain(">-0<")
    expect(html).toContain("tool-timeline-pierre-root-patch")
  })

  test("renders shell details from streaming output segments", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "Shell",
      arguments: {
        command: "cargo test --workspace",
        description: "Runs workspace tests",
      },
      outputContent: "",
      outputSegments: [
        { stream: "stdout", text: "running tests\n" },
        { stream: "stderr", text: "warning: noisy\n" },
        { stream: "stdout", text: "all passed\n" },
      ],
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("tool-timeline-shell-command")
    expect(html).toContain("tool-timeline-shell-segment-stdout")
    expect(html).toContain("tool-timeline-shell-segment-stderr")
    expect(html).toContain("$ cargo test --workspace")
    expect(html).toContain("running tests")
    expect(html).toContain("warning: noisy")
    expect(html).toContain("all passed")
  })

  test("keeps shell output auto-follow logic inside renderer-owned component", () => {
    const source = readFileSync(
      new URL("./renderers/shell.tsx", import.meta.url),
      "utf8"
    ).replace(/\s+/g, " ")

    expect(source).toContain("function ShellOutputBody(")
    expect(source).toContain(
      "const preRef = useRef<HTMLPreElement | null>(null)"
    )
    expect(source).toContain("shouldFollowRef.current = distance <= 12")
    expect(source).toContain("element.scrollTop = element.scrollHeight")
  })

  test("auto-scrolls running shell output to latest line when details mount", () => {
    const source = readFileSync(
      new URL("./renderers/shell.tsx", import.meta.url),
      "utf8"
    ).replace(/\s+/g, " ")

    expect(source).toContain("isRunning: boolean")
    expect(source).toContain("if (isRunning) {")
    expect(source).toContain("shouldFollowRef.current = true")
    expect(source).toContain("element.scrollTop = element.scrollHeight")
    expect(source).toContain("isRunning={data.isRunning ?? false}")
  })

  test("passes streaming shell segments through tool details panel", () => {
    const source = readFileSync(
      new URL("../tool-timeline/tool-details-panel.tsx", import.meta.url),
      "utf8"
    ).replace(/\s+/g, " ")

    expect(source).toContain("outputSegments: item.outputSegments")
    expect(source).toContain("isRunning: item.finishedAtMs == null")
  })

  test("renders ApplyPatch move detail with basename-first hierarchy", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "ApplyPatch",
      arguments: {
        patch:
          "*** Begin Patch\n*** Update File: src/old/file.ts\n*** Move to: app/new/file-renamed.ts\n@@\n-old\n+new\n*** End Patch",
      },
      outputContent:
        "*** Begin Patch\n*** Update File: src/old/file.ts\n*** Move to: app/new/file-renamed.ts\n@@\n-old\n+new\n*** End Patch",
      succeeded: true,
    })

    const html = renderWithTheme(details)
    expect(html).toContain("file.ts → file-renamed.ts")
    expect(html).toContain("src/old/ → app/new/")
    expect(html).toContain('class="tool-timeline-patch-filename"')
    expect(html).toContain('class="tool-timeline-patch-directory"')
    expect(html).toContain(
      'class="tool-timeline-patch-filename">file.ts → file-renamed.ts<span class="tool-timeline-patch-directory"> \u202Asrc/old/ → app/new/\u202C</span></span>'
    )
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

  test("renders ApplyPatch multi-file subtitle as first path plus count", () => {
    const subtitle = toolRendererRegistry.renderSubtitle({
      toolName: "ApplyPatch",
      arguments: {
        patch: [
          "*** Begin Patch",
          "*** Update File: old.txt",
          "*** Move to: nested/new.txt",
          "@@",
          "-old",
          "+new",
          "*** Add File: another.txt",
          "+hello",
          "*** End Patch",
        ].join("\n"),
      },
      outputContent: "",
      succeeded: true,
    })

    expect(subtitle).toBe("old.txt → nested/new.txt +1 files")
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

  test("renders question title from partial structured questions while waiting", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Question",
      arguments: {
        questions: [
          {
            question: "你晚上吃了的话，第一反应会想吃什么？",
          },
        ],
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("你晚上吃了的话，第一反应会想吃什么？")
  })

  test("renders multi-question results as a compact answered list", () => {
    const title = toolRendererRegistry.renderTitle({
      toolName: "Question",
      arguments: {
        questions: [
          {
            id: "q1",
            question: "你最近大概处于哪种状态？",
            kind: "choice",
          },
          {
            id: "q2",
            question: "你更喜欢哪类工作？",
            kind: "choice",
          },
        ],
      },
      details: {
        status: "answered",
        answers: [
          {
            question_id: "q1",
            text: "在冲项目",
            selected_option_ids: [],
          },
        ],
      },
      outputContent: "",
      succeeded: true,
    })

    const subtitle = toolRendererRegistry.renderSubtitle({
      toolName: "Question",
      arguments: {
        questions: [
          {
            id: "q1",
            question: "你最近大概处于哪种状态？",
            kind: "choice",
          },
          {
            id: "q2",
            question: "你更喜欢哪类工作？",
            kind: "choice",
          },
        ],
      },
      details: {
        status: "answered",
        answers: [
          {
            question_id: "q1",
            text: "在冲项目",
            selected_option_ids: [],
          },
        ],
      },
      outputContent: "",
      succeeded: true,
    })

    const meta = toolRendererRegistry.renderMeta({
      toolName: "Question",
      arguments: {
        questions: [
          {
            id: "q1",
            question: "你最近大概处于哪种状态？",
            kind: "choice",
          },
          {
            id: "q2",
            question: "你更喜欢哪类工作？",
            kind: "choice",
          },
        ],
      },
      details: {
        status: "answered",
        answers: [
          {
            question_id: "q1",
            text: "在冲项目",
            selected_option_ids: [],
          },
        ],
      },
      outputContent: "",
      succeeded: true,
    })

    const details = toolRendererRegistry.renderDetails({
      toolName: "Question",
      arguments: {
        questions: [
          {
            id: "q1",
            question: "你最近大概处于哪种状态？",
            kind: "choice",
          },
          {
            id: "q2",
            question: "你更喜欢哪类工作？",
            kind: "choice",
          },
        ],
      },
      details: {
        status: "answered",
        answers: [
          {
            question_id: "q1",
            text: "在冲项目",
            selected_option_ids: [],
          },
        ],
      },
      outputContent: "",
      succeeded: true,
    })

    expect(title).toBe("Questions")
    expect(subtitle).toBe("1 answered")
    expect(meta).toBe(null)
    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("你最近大概处于哪种状态？")
    expect(html).toContain("在冲项目")
    expect(html).toContain("你更喜欢哪类工作？")
    expect(html).toContain("(no answer)")
    expect(html).toContain("text-muted-foreground/78")
    expect(html).toContain("leading-6 font-medium text-foreground")
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

  test("deduplicates fallback result when output JSON matches structured details", () => {
    const details = toolRendererRegistry.renderDetails({
      toolName: "tape_info",
      arguments: {},
      details: {
        anchors: 0,
        context_limit: 340000,
        entries: 48,
      },
      outputContent: '{"entries":48,"anchors":0,"context_limit":340000}',
      succeeded: true,
    })

    expect(details).not.toBe(null)
    const html = renderToStaticMarkup(<>{details}</>)
    expect(html).toContain("Raw Details")
    expect(html).not.toContain("Result")
    expect(html).not.toContain("Expand")
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

  test("keeps pierre diff host background transparent", () => {
    const source = loadPierreConfigSource()

    expect(source).toContain('background: "transparent"')
    expect(source).toContain('"--aia-diff-surface": "transparent"')
    expect(source).toContain('"--diffs-bg-buffer-override": "transparent"')
    expect(source).toContain('"--diffs-bg-hover-override": "transparent"')
  })

  test("keeps pierre virtualizer local to concrete diff renderers", () => {
    const source = loadPierreDiffSource()
    const configSource = loadPierreConfigSource()

    expect(source).toContain("Virtualizer")
    expect(source).toContain('className="tool-timeline-pierre-virtualizer"')
    expect(source).toContain(
      'contentClassName="tool-timeline-pierre-virtualizer-content"'
    )
    expect(configSource).toContain("overscrollSize: 1200")
    expect(configSource).toContain("intersectionObserverMargin: 600")
    expect(source).toContain('lineDiffType: "none"')
    expect(source).not.toContain("WorkerPoolContextProvider")
  })

  test("keeps shared pierre worker pool configuration in a dedicated provider module", () => {
    const source = loadPierreDiffProviderSource()

    expect(source).toContain("WorkerPoolContextProvider")
    expect(source).toContain("pierreWorkerPoolOptions")
    expect(source).toContain('preferredHighlighter: "shiki-js"')
    expect(source).toContain('lineDiffType: "none"')
    expect(source).toContain("highlighterOptions")
    expect(source).not.toContain("sharedRenderOptions")
  })

  test("injects pierre diff scrollbar hiding hooks via unsafe css", () => {
    const source = loadPierreConfigSource()

    expect(source).toContain("scrollbar-width: none;")
    expect(source).toContain("-ms-overflow-style: none;")
    expect(source).toContain(":host ::-webkit-scrollbar {")
    expect(source).toContain("height: 0;")
    expect(source).toContain("display: none;")
  })

  test("keeps pierre diff host border on both horizontal edges", () => {
    const source = loadAppIndexCssSource()

    expect(source).toContain(".tool-timeline-pierre-root")
    expect(source).toContain(
      "border: 1px solid color-mix(in oklch, var(--border) 58%, transparent);"
    )
    expect(source).not.toContain(
      ".tool-timeline-pierre-root { display: block; width: 100%; overflow: hidden; border-radius: 0.875rem; border: 1px solid color-mix(in oklch, var(--border) 58%, transparent); border-left: 0;"
    )
  })
})
