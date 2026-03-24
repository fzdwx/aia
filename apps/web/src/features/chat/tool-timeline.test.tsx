import { readFileSync } from "node:fs"
import type { ReactElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import { ThemeProvider } from "@/components/theme-provider"

import { buildDetailEntries } from "./tool-rendering/helpers"
import { ExpandableOutput, ToolDetailSection } from "./tool-rendering/ui"
import { StreamingToolGroup, ToolGroup } from "./tool-timeline"
import { isContextExplorationTool } from "./tool-timeline-helpers"

function renderWithTheme(content: ReactElement) {
  return renderToStaticMarkup(
    <ThemeProvider defaultTheme="dark">{content}</ThemeProvider>
  )
}

function loadToolTimelineSource() {
  return readFileSync(
    new URL("./tool-timeline.tsx", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

describe("tool timeline", () => {
  test("normalizes names when classifying context exploration tools", () => {
    expect(isContextExplorationTool("functions.read")).toBe(true)
    expect(isContextExplorationTool("grep")).toBe(true)
    expect(isContextExplorationTool("list")).toBe(true)
    expect(isContextExplorationTool("codesearch")).toBe(true)
    expect(isContextExplorationTool("websearch")).toBe(true)
    expect(isContextExplorationTool("shell")).toBe(false)
  })

  test("builds compact request and result entries for structured tool details", () => {
    const requestEntries = buildDetailEntries(
      {
        file_path: "apps/web/src/components/chat-messages.tsx",
        offset: 120,
        limit: 40,
        recursive: true,
      },
      { omitKeys: ["file_path"] }
    )
    const resultEntries = buildDetailEntries({
      lines_read: 2,
      total_lines: 220,
      truncated: false,
    })

    expect(requestEntries).toEqual([
      { label: "Offset", value: 120 },
      { label: "Limit", value: 40 },
      { label: "Recursive", value: "yes" },
    ])
    expect(resultEntries).toEqual([
      { label: "Lines Read", value: 2 },
      { label: "Total Lines", value: 220 },
      { label: "Truncated", value: "no" },
    ])
  })

  test("renders running context groups with status title and context trigger rows", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-1",
            toolName: "grep",
            arguments: {
              pattern: "renderDetails",
              path: "apps/web/src",
            },
            detectedAtMs: Date.now() - 100,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("Exploring")
    expect(html).toContain("grep")
    expect(html).toContain("renderDetails")
    expect(html).toContain('data-component="context-tool-trigger-row"')
    expect(html).toContain('data-slot="tool-title"')
    expect(html).toContain('data-slot="tool-subtitle"')
    expect(html).toMatch(
      /data-component="tool-group"[\s\S]*data-component="context-tool-group-trigger"[\s\S]*Exploring/
    )
    expect(html).not.toContain("Running tools")
  })

  test("renders completed context groups with status and summary counts", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-list-1",
            toolName: "list",
            arguments: {
              path: "apps/web/src/components",
            },
            startedAtMs: 80,
            finishedAtMs: 90,
            succeeded: true,
            outputContent: "component-a\ncomponent-b",
            details: {
              path: "apps/web/src/components",
              count: 2,
            },
          },
          {
            id: "tool-grep-1",
            toolName: "grep",
            arguments: {
              pattern: "renderDetails",
              path: "apps/web/src",
            },
            startedAtMs: 91,
            finishedAtMs: 95,
            succeeded: true,
            outputContent: "renderDetails",
            details: {
              matches: 1,
            },
          },
          {
            id: "tool-read-1",
            toolName: "read",
            arguments: {
              file_path: "apps/web/src/components/chat-messages.tsx",
              offset: 120,
              limit: 40,
            },
            startedAtMs: 100,
            finishedAtMs: 220,
            succeeded: true,
            outputContent: Array.from(
              { length: 12 },
              (_, index) => `${index + 1}`
            ).join("\n"),
            details: {
              lines_read: 12,
              total_lines: 240,
            },
          },
        ]}
      />
    )

    expect(html).toContain('aria-expanded="false"')
    expect(html).toContain("Explored")
    expect(html).toContain("1 read")
    expect(html).toContain("1 search")
    expect(html).toContain("1 list")
    expect(html).not.toContain("Running")
  })

  test("reads shared timeline copy instead of hardcoded group labels", () => {
    const source = loadToolTimelineSource()

    expect(source).toContain(
      'import { toolTimelineCopy } from "./tool-timeline-copy"'
    )
    expect(source).toContain("toolTimelineCopy.groupStatus.running")
    expect(source).toContain("toolTimelineCopy.groupStatus.completed")
    expect(source).toContain("contextCount")
    expect(source).toContain("toolTimelineCopy")
    expect(source).not.toContain('running: "Exploring"')
    expect(source).not.toContain('completed: "Explored"')
  })

  test("renders standalone tools without context-group titles", () => {
    const completedHtml = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-shell-1",
            toolName: "shell",
            arguments: {
              command: "cargo check",
            },
            startedAtMs: 100,
            finishedAtMs: 220,
            succeeded: true,
            outputContent: "ok",
            details: {},
          },
        ]}
      />
    )
    const runningHtml = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-shell-1",
            toolName: "shell",
            arguments: {
              command: "cargo check",
            },
            detectedAtMs: Date.now() - 100,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(completedHtml).toContain("shell")
    expect(completedHtml).not.toContain("Explored")
    expect(completedHtml).not.toContain("Exploring")

    expect(runningHtml).toContain("shell")
    expect(runningHtml).not.toContain("Explored")
    expect(runningHtml).not.toContain("Exploring")
    expect(runningHtml).not.toContain("Running tools")
  })

  test("hides pending question tools until they have a stable outcome", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-question-1",
            toolName: "functions.question",
            arguments: {
              question: "Override existing config?",
            },
            detectedAtMs: Date.now() - 100,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).not.toContain("Override existing config?")
    expect(html).not.toContain('data-component="tool-row"')
  })

  test("renders english fallbacks for empty outputs and silent failures", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-shell-empty",
            toolName: "shell",
            arguments: {},
            startedAtMs: 100,
            finishedAtMs: 140,
            succeeded: true,
            outputContent: "",
          },
          {
            id: "tool-shell-failed-empty",
            toolName: "shell",
            arguments: {},
            startedAtMs: 150,
            finishedAtMs: 190,
            succeeded: false,
            outputContent: "",
          },
        ]}
      />
    )

    expect(html).toContain("No output returned.")
    expect(html).toContain("Tool execution failed.")
  })

  test("coalesces out-of-order streaming tools into one completed row", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-out-of-order-1",
            toolName: "",
            arguments: {},
            detectedAtMs: 0,
            output: "",
            completed: true,
            finishedAtMs: 220,
            resultContent: "final result",
          },
          {
            invocationId: "streaming-out-of-order-1",
            toolName: "shell",
            arguments: {
              command: "cargo check",
            },
            detectedAtMs: 120,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("cargo check")
    expect(html).not.toContain("Exploring")
    expect(html.match(/data-component="tool-row-trigger"/g)).toHaveLength(1)
  })

  test("uses data-component attributes for context groups and tool rows", () => {
    const source = loadToolTimelineSource()

    expect(source).toContain(
      "const isContextGroup = visibleItems.every((item) =>"
    )
    expect(source).toContain("isContextExplorationTool(item.toolName)")
    expect(source).toContain('const isRunning = status === "running"')
    expect(source).toContain("const [open, setOpen] = useState(isRunning)")
    expect(source).toContain(
      'data-component="tool-group" data-variant="standalone"'
    )
    expect(source).toContain('data-component="context-tool-group-trigger"')
    expect(source).toContain("setOpen((current) => !current)")
    expect(source).toContain('status="running"')
  })

  test("uses a fixed tween transition for detail panels to avoid spring jitter", () => {
    const source = loadToolTimelineSource()

    expect(source).toContain("const TOOL_DETAILS_TRANSITION =")
    expect(source).toContain("duration: 0.18")
    expect(source).toContain("ease: [0.16, 1, 0.3, 1]")
    expect(source).toContain('ease: "linear"')
    expect(source).toContain("transition={TOOL_DETAILS_TRANSITION}")
    expect(source).not.toContain('type: "spring"')
  })

  test("renders edit tools as expandable rows without a caret icon", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-edit-inline",
            toolName: "edit",
            arguments: {
              file_path: "apps/web/src/index.css",
            },
            startedAtMs: 100,
            finishedAtMs: 220,
            succeeded: true,
            outputContent: "@@\n-old\n+new",
            details: {
              added: 1,
              removed: 1,
              diff: "@@\n-old\n+new",
            },
          },
        ]}
      />
    )

    expect(html).toContain('data-slot="tool-meta"')
    expect(html).toContain("+1")
    expect(html).toContain("-1")
    expect(html).toContain('aria-expanded="false"')
    expect(html).not.toContain('data-slot="tool-row-caret"')
    expect(html).not.toContain('data-slot="tool-row-inline-details"')
  })

  test("keeps write and apply_patch tools on the inline detail path", () => {
    const source = loadToolTimelineSource()

    expect(source).toContain(
      'const INLINE_DETAIL_TOOLS = new Set(["write", "apply_patch"])'
    )
    expect(source).toContain("shouldShowToolRowCaret(item, hasDetails)")
  })

  test("renders semantic containers for output, failure, and patch sections", () => {
    const html = renderWithTheme(
      <div>
        <ToolDetailSection title="Content">
          <ExpandableOutput value="ok" failed={false} />
        </ToolDetailSection>
        <ToolDetailSection title="Failure">
          <ExpandableOutput value="error" failed />
        </ToolDetailSection>
        <ToolDetailSection title="Patch">
          <ExpandableOutput
            value="*** Update File: apps/web/src/index.css"
            failed={false}
          />
        </ToolDetailSection>
      </div>
    )

    expect(html).toContain('data-tool-detail-kind="content"')
    expect(html).toContain('data-tool-detail-tone="output"')
    expect(html).toContain('data-tool-detail-kind="failure"')
    expect(html).toContain('data-tool-detail-tone="failure"')
    expect(html).toContain('data-tool-detail-kind="patch"')
    expect(html).toContain('data-tool-detail-tone="patch"')
    expect(html).toContain("tool-timeline-output-pre")
  })
})
