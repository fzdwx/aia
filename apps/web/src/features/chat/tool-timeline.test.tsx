import { readFileSync } from "node:fs"
import type { ReactElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import { ThemeProvider } from "@/components/theme-provider"

import { buildDetailEntries } from "./tool-rendering/helpers"
import { ExpandableOutput, ToolDetailSection } from "./tool-rendering/ui"
import { StreamingToolGroup, ToolGroup } from "./tool-timeline"
import { renderToolDetailsPanel } from "./tool-timeline/tool-details-panel"
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

function loadToolRowPolicySource() {
  return readFileSync(
    new URL("./tool-timeline/tool-row-policy.ts", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadToolDetailsPanelSource() {
  return readFileSync(
    new URL("./tool-timeline/tool-details-panel.tsx", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadContextGroupSource() {
  return readFileSync(
    new URL("./tool-timeline/context-group.tsx", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadWebCssSource() {
  return readFileSync(
    new URL("../../index.css", import.meta.url),
    "utf8"
  ).replace(/\s+/g, " ")
}

function loadViteConfigSource() {
  return readFileSync(
    new URL("../../../vite.config.ts", import.meta.url),
    "utf8"
  )
}

describe("tool timeline", () => {
  test("normalizes names when classifying context exploration tools", () => {
    expect(isContextExplorationTool("Read")).toBe(true)
    expect(isContextExplorationTool("Grep")).toBe(true)
    expect(isContextExplorationTool("list")).toBe(true)
    expect(isContextExplorationTool("CodeSearch")).toBe(true)
    expect(isContextExplorationTool("CodeSearch")).toBe(true)
    expect(isContextExplorationTool("WebSearch")).toBe(true)
    expect(isContextExplorationTool("Shell")).toBe(false)
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
        keepContextGroupsOpen
        toolOutputs={[
          {
            invocationId: "streaming-1",
            toolName: "Grep",
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
    expect(html).toContain("Grep")
    expect(html).toContain("&quot;renderDetails&quot; apps/web/src")
    expect(html).toContain('data-component="context-tool-trigger-row"')
    expect(html).toContain('data-slot="tool-title"')
    expect(html).toContain('data-slot="tool-subtitle"')
    expect(html).toMatch(
      /data-component="tool-group"[\s\S]*data-component="context-tool-group-trigger"[\s\S]*Exploring/
    )
    expect(html).not.toContain("Running tools")
  })

  test("keeps completed explored groups expanded while the turn is still streaming", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        keepContextGroupsOpen
        toolOutputs={[
          {
            invocationId: "completed-list-1",
            toolName: "list",
            arguments: {
              path: "apps/web/src/components",
            },
            detectedAtMs: 80,
            output: "component-a\ncomponent-b",
            completed: true,
            finishedAtMs: 90,
            resultContent: "component-a\ncomponent-b",
          },
          {
            invocationId: "active-grep-1",
            toolName: "Grep",
            arguments: {
              pattern: "renderDetails",
              path: "apps/web/src",
            },
            detectedAtMs: 100,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain('aria-expanded="true"')
    expect(html).toContain('data-component="context-tool-group-list"')
    expect(html).toContain("apps/web/src/components")
    expect(html).toContain("renderDetails")
  })

  test("renders read and grep context meta in explored groups", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        keepContextGroupsOpen
        toolOutputs={[
          {
            invocationId: "completed-read-1",
            toolName: "Read",
            arguments: {
              file_path: "apps/web/src/components/chat-messages.tsx",
              offset: 120,
              limit: 40,
            },
            detectedAtMs: 80,
            output: "line 121\nline 122",
            completed: true,
            finishedAtMs: 90,
            resultContent: "line 121\nline 122",
            resultDetails: {
              lines_read: 40,
              total_lines: 240,
            },
          },
          {
            invocationId: "completed-grep-1",
            toolName: "Grep",
            arguments: {
              pattern: "renderDetails",
              path: "apps/web/src",
            },
            detectedAtMs: 100,
            output: "renderDetails",
            completed: true,
            finishedAtMs: 110,
            resultContent: "renderDetails",
            resultDetails: {
              matches: 12,
              returned: 5,
              truncated: true,
            },
          },
          {
            invocationId: "active-list-1",
            toolName: "list",
            arguments: {
              path: "apps/web/src/components",
            },
            detectedAtMs: 120,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain('data-slot="tool-meta"')
    expect(html).toContain("chat-messages.tsx")
    expect(html).toContain("apps/web/src/components")
    expect(html).toContain('data-slot="tool-file-name"')
    expect(html).toContain('data-slot="tool-file-dir"')
    expect(html).toContain("L121~160")
    expect(html).toContain("&quot;renderDetails&quot; apps/web/src")
    expect(html).toContain("12 matches")
    expect(html).toContain("showing 5")
  })

  test("renders read context range from total lines without raw offset and limit args", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        keepContextGroupsOpen
        toolOutputs={[
          {
            invocationId: "completed-read-range-1",
            toolName: "Read",
            arguments: {
              file_path: "apps/web/src/components/chat-messages.tsx",
              offset: 0,
              limit: 220,
            },
            detectedAtMs: 80,
            output: "line 1\nline 2",
            completed: true,
            finishedAtMs: 90,
            resultContent: "line 1\nline 2",
            resultDetails: {
              total_lines: 240,
            },
          },
          {
            invocationId: "active-grep-range-1",
            toolName: "Grep",
            arguments: {
              pattern: "renderDetails",
              path: "apps/web/src",
            },
            detectedAtMs: 100,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("L1~240")
    expect(html).not.toContain("offset=0")
    expect(html).not.toContain("limit=220")
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
            toolName: "Grep",
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
            toolName: "Read",
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
    expect(html).toContain('data-state="visible"')
    expect(html).not.toContain('data-component="context-tool-group-list"')
    expect(html).not.toContain("Running")
  })

  test("reads shared timeline copy instead of hardcoded group labels", () => {
    const source = loadContextGroupSource()

    expect(source).toContain(
      'import { toolTimelineCopy } from "@/features/chat/tool-timeline-copy"'
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
            toolName: "Shell",
            arguments: {
              command: "cargo check",
              description: "Run workspace checks",
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
            toolName: "Shell",
            arguments: {
              command: "cargo check",
              description: "Run workspace checks",
            },
            detectedAtMs: Date.now() - 100,
            startedAtMs: Date.now() - 90,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(completedHtml).toContain("Shell")
    expect(completedHtml).toContain("Run workspace checks")
    expect(completedHtml).not.toContain('data-slot="tool-row-caret"')
    expect(completedHtml).not.toContain("Explored")
    expect(completedHtml).not.toContain("Exploring")

    expect(runningHtml).toContain("Shell")
    expect(runningHtml).toContain("Run workspace checks")
    expect(runningHtml).toContain('data-slot="tool-duration"')
    expect(runningHtml).not.toContain("Explored")
    expect(runningHtml).not.toContain("Exploring")
    expect(runningHtml).not.toContain("Running tools")
  })

  test("renders running tools with live subtitle and duration", () => {
    const now = Date.now()
    const shellHtml = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-shell-live-1",
            toolName: "Shell",
            arguments: {
              command: "cargo check",
              description: "Run workspace checks",
            },
            detectedAtMs: now - 1500,
            startedAtMs: now - 1200,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    const readHtml = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-read-live-1",
            toolName: "Read",
            arguments: {
              file_path: "apps/web/src/features/chat/tool-timeline.tsx",
              offset: 1,
              limit: 20,
            },
            detectedAtMs: now - 1800,
            startedAtMs: now - 1000,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(shellHtml).toContain("Run workspace checks")
    expect(shellHtml).toContain('data-slot="tool-subtitle"')
    expect(shellHtml).toContain('data-slot="tool-duration"')
    expect(readHtml).toContain("tool-timeline.tsx")
    expect(readHtml).toContain('data-slot="tool-file-name"')
    expect(readHtml).toContain('data-slot="tool-subtitle"')
    expect(readHtml).toContain('data-slot="tool-meta"')
    expect(readHtml).toContain("L2~21")
  })

  test("renders running shell subtitle directly from started-event arguments", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-shell-args-1",
            toolName: "Shell",
            arguments: {
              command: "cargo test --workspace",
              description: "Runs Rust workspace tests again",
            },
            detectedAtMs: Date.now() - 3000,
            startedAtMs: Date.now() - 2500,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("Shell")
    expect(html).toContain("Runs Rust workspace tests again")
    expect(html).toContain('data-slot="tool-subtitle"')
    expect(html).toContain('data-slot="tool-duration"')
  })

  test("prefers shell description over first output line in row subtitles", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-shell-subtitle-1",
            toolName: "Shell",
            arguments: {
              command: "pnpm run test",
              description: "running 32 tests",
            },
            detectedAtMs: Date.now() - 41_100,
            startedAtMs: Date.now() - 41_000,
            output: "aia@0.0.1 test /home/like/projects/aia/apps/web",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("running 32 tests")
    expect(html).not.toContain(
      ">aia@0.0.1 test /home/like/projects/aia/apps/web<"
    )
  })

  test("does not show live duration before a shell actually starts", () => {
    const now = Date.now()
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-shell-started-1",
            toolName: "Shell",
            arguments: {
              command: "cargo test",
              description: "Runs Rust workspace tests",
            },
            detectedAtMs: now - 16_400,
            startedAtMs: now - 16_300,
            output: "",
            completed: false,
          },
          {
            invocationId: "streaming-shell-detected-2",
            toolName: "Shell",
            arguments: {
              command: "pnpm run check",
              description: "Runs frontend full checks",
            },
            detectedAtMs: now - 16_400,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("Runs Rust workspace tests")
    expect(html).toContain("Runs frontend full checks")
    expect(html.match(/data-slot="tool-duration"/g)?.length ?? 0).toBe(1)
  })

  test("uses per-shell startedAtMs for parallel running durations", () => {
    const now = Date.now()
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "parallel-shell-1",
            toolName: "Shell",
            arguments: {
              command: "pnpm run test",
              description: "running 32 tests",
            },
            detectedAtMs: now - 41_100,
            startedAtMs: now - 41_100,
            output: "aia@0.0.1 test /home/like/projects/aia/apps/web",
            completed: false,
          },
          {
            invocationId: "parallel-shell-2",
            toolName: "Shell",
            arguments: {
              command: "pnpm run test",
              description: "running 32 tests",
            },
            detectedAtMs: now - 41_100,
            startedAtMs: now - 2_100,
            output: "aia@0.0.1 test /home/like/projects/aia/apps/web",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("41.1 s")
    expect(html).toContain("2.1 s")
  })

  test("does not show live duration for shell before startedAtMs arrives", () => {
    const now = Date.now()
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-shell-pending-1",
            toolName: "Shell",
            arguments: {
              command: "pnpm run check",
              description: "Runs frontend full checks",
            },
            detectedAtMs: now - 16_400,
            output: "",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("Runs frontend full checks")
    expect(html).not.toContain('data-slot="tool-duration"')
  })

  test("renders running fallback tools with first output line as subtitle", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-unknown-live-1",
            toolName: "CustomTool",
            arguments: {},
            detectedAtMs: Date.now() - 2000,
            output: "first useful line\nsecond line",
            completed: false,
          },
        ]}
      />
    )

    expect(html).toContain("CustomTool")
    expect(html).toContain("first useful line")
    expect(html).toContain('data-slot="tool-subtitle"')
    expect(html).toContain('data-slot="tool-duration"')
  })

  test("renders completed fallback tools with first output line as subtitle", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-unknown-complete-1",
            toolName: "Shell",
            arguments: {},
            detectedAtMs: Date.now() - 3000,
            finishedAtMs: Date.now() - 100,
            output: "",
            resultContent: "Runs frontend full checks again\nall good",
            completed: true,
          },
        ]}
      />
    )

    expect(html).toContain("Shell")
    expect(html).toContain("Runs frontend full checks again")
    expect(html).toContain('data-slot="tool-subtitle"')
  })

  test("renders TapeInfo as a non-expandable inline summary row", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-tape-info-1",
            toolName: "TapeInfo",
            arguments: {},
            startedAtMs: 100,
            finishedAtMs: 140,
            succeeded: true,
            outputContent:
              '{"total_entries":12,"anchor_count":1,"entries_since_last_anchor":4,"pressure_ratio":0.7}',
            details: {
              total_entries: 12,
              anchor_count: 1,
              entries_since_last_anchor: 4,
              pressure_ratio: 0.7,
            },
          },
        ]}
      />
    )

    expect(html).toContain("TapeInfo")
    expect(html).toContain("pressure 70.0%")
    expect(html).toContain("12 entries")
    expect(html).toContain("1 anchor")
    expect(html).toContain("+4 since anchor")
    expect(html).not.toContain("&quot;total_entries&quot;")
    expect(html).not.toContain("aria-expanded")
    expect(html).not.toContain('data-slot="tool-row-details"')
  })

  test("renders TapeHandoff details without generic or summary headings", () => {
    const html = renderWithTheme(
      <>
        {renderToolDetailsPanel({
          id: "tool-tape-handoff-1",
          toolName: "TapeHandoff",
          arguments: {
            name: "session-cut",
            summary: "Condensed handoff summary for the next wake.",
          },
          startedAtMs: 100,
          finishedAtMs: 140,
          succeeded: true,
          outputContent: [
            "## Resume plan",
            "",
            "- Carry over unresolved TODO items.",
            "- Resume from anchor `A42`.",
          ].join("\n"),
          details: {
            summary: "Condensed handoff summary for the next wake.",
          },
        })}
      </>
    )

    expect(html).toContain("Condensed handoff summary for the next wake.")
    expect(html).toContain("tool-timeline-shell-detail")
    expect(html).toContain("tool-timeline-shell-body")
    expect(html).toContain("markdown-content")
    expect(html).toContain("Resume plan")
    expect(html).toContain("Carry over unresolved TODO items.")
    expect(html).toContain("Resume from anchor")
    expect(html).not.toContain("Input")
    expect(html).not.toContain("Result")
    expect(html).not.toContain("Content")
    expect(html).not.toContain('tool-timeline-detail-title">Summary')
  })

  test("keeps fallback tool detail rendering on the renderer-owned path", () => {
    const source = loadToolDetailsPanelSource()

    expect(source).toContain(
      "const usesDefaultRenderer = !NON_DEFAULT_TOOL_NAMES.has(normalizedToolName)"
    )
    expect(source).toContain(
      "if (usesDefaultRenderer && detailsContent != null)"
    )
    expect(source).toContain(
      "<ToolDetailSurface>{detailsContent}</ToolDetailSurface>"
    )
  })

  test("keeps explored list spacing inside the measured inner content", () => {
    const source = loadWebCssSource()

    expect(source).toContain('[data-component="tool-group"]')
    expect(source).toContain("padding-top: 0.125rem")
    expect(source).toContain("padding-bottom: 0.125rem")
    expect(source).toContain(
      '[data-component="tool-group"][data-variant="standalone"]'
    )
    expect(source).toContain("gap: 0.25rem")
    expect(source).toContain('[data-component="context-tool-group-list"]')
    expect(source).toContain("padding-top: 0;")
    expect(source).toContain('[data-slot="context-tool-group-list-inner"]')
    expect(source).toContain("padding-top: 0.375rem")
  })

  test("uses the same subdued color for tool meta and subtitles", () => {
    const source = loadWebCssSource()

    expect(source).toContain('[data-slot="tool-subtitle"]')
    expect(source).toContain(
      '[data-slot="tool-subtitle"][data-kind="file-path"]'
    )
    expect(source).toContain(
      "grid-template-columns: minmax(0, 15ch) minmax(0, 1fr);"
    )
    expect(source).toContain('[data-slot="tool-meta"]')
    expect(source).toContain(
      "color: oklch(from var(--muted-foreground) l c h / 0.5);"
    )
  })

  test("styles ApplyPatch file items with filename-first hierarchy", () => {
    const source = loadWebCssSource()

    expect(source).toContain(".tool-timeline-patch-path")
    expect(source).toContain("flex-direction: column")
    expect(source).toContain(".tool-timeline-patch-filename")
    expect(source).toContain("font-weight: 500")
    expect(source).toContain(".tool-timeline-patch-directory")
    expect(source).toContain(
      "color: color-mix(in oklch, var(--text-weak) 82%, transparent);"
    )
    expect(source).toContain(".tool-timeline-patch-stat")
  })

  test("styles pierre virtualizer as the scroll container", () => {
    const source = loadWebCssSource()

    expect(source).toContain(".tool-timeline-pierre-virtualizer")
    expect(source).toContain("max-height: min(32rem, 70vh)")
    expect(source).toContain("overflow: auto")
    expect(source).toContain("overscroll-behavior: contain")
    expect(source).toContain(".tool-timeline-pierre-virtualizer-content")
    expect(source).toContain("min-width: 100%")
  })

  test("configures Vite workers for Pierre diff highlighting", () => {
    const source = loadViteConfigSource()

    expect(source).toContain("worker:")
    expect(source).toContain('format: "es"')
  })

  test("uses default cursor for timeline tool triggers", () => {
    const source = loadWebCssSource()

    expect(source).toContain(
      'button[data-component="context-tool-group-trigger"]'
    )
    expect(source).toContain('[data-component="tool-row-trigger"]')
    expect(source).toContain(".tool-timeline-output-toggle")
    expect(source).toContain(".tool-timeline-info-trigger,")
    expect(source).toContain(".tool-timeline-json-trigger {")
    expect(source).toContain("cursor: default;")
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

  test("hides pending uppercase Question tools until they have a stable outcome", () => {
    const html = renderWithTheme(
      <StreamingToolGroup
        toolOutputs={[
          {
            invocationId: "streaming-Question-1",
            toolName: "Question",
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

  test("renders uppercase Question details without generic Input section", () => {
    const html = renderWithTheme(
      <>
        {renderToolDetailsPanel({
          id: "tool-question-1",
          toolName: "Question",
          arguments: {
            questions: [
              {
                id: "q1",
                question: "你最近大概处于哪种状态？",
                kind: "choice",
              },
            ],
          },
          startedAtMs: 100,
          finishedAtMs: 140,
          succeeded: true,
          outputContent: "",
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
        })}
      </>
    )

    expect(html).toContain("你最近大概处于哪种状态？")
    expect(html).toContain("在冲项目")
    expect(html).not.toContain("Input")
  })

  test("renders english fallbacks for empty outputs and silent failures", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-shell-empty",
            toolName: "Shell",
            arguments: {},
            startedAtMs: 100,
            finishedAtMs: 140,
            succeeded: true,
            outputContent: "",
          },
          {
            id: "tool-shell-failed-empty",
            toolName: "Shell",
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
            toolName: "Shell",
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
    const contextSource = loadContextGroupSource()

    expect(source).toContain(
      "const isContextGroup = visibleItems.every((item) =>"
    )
    expect(source).toContain("isContextExplorationTool(item.toolName)")
    expect(source).toContain('const isRunning = status === "running"')
    expect(source).toContain(
      'data-component="tool-group" data-variant="standalone"'
    )
    expect(source).toContain("setOpen((current) => !current)")
    expect(source).toContain('status="running"')
    expect(source).toContain("keepContextGroupsOpen = false")
    expect(source).toContain(
      "const shouldKeepOpen = isContextGroup && (isRunning || keepContextGroupsOpen)"
    )
    expect(source).toContain("if (!isContextGroup) {")
    expect(source).toContain("} else if (wasOpenRef.current) {")
    expect(source).toContain("setOpen(false)")
    expect(source).toContain("keepContextGroupsOpen?: boolean")
    expect(contextSource).toContain(
      'data-component="context-tool-group-trigger"'
    )
    expect(contextSource).toContain('data-slot="context-group-counts-shell"')
    expect(contextSource).toContain('data-state={open ? "hidden" : "visible"}')
    expect(contextSource).toContain("function ContextToolGroupList(")
    expect(contextSource).toContain('animate={{ height: "auto" }}')
    expect(contextSource).toContain("const CONTEXT_GROUP_TRANSITION =")
  })

  test("keeps expanded shell rows controlled across active to completed regrouping", () => {
    const source = loadToolTimelineSource()

    expect(source).toContain(
      "const [expandedToolIds, setExpandedToolIds] = useState"
    )
    expect(source).toContain("expandedToolIds={expandedToolIds}")
    expect(source).toContain("onExpandedChange={handleExpandedChange}")
    expect(source).toContain("expanded={expandedToolIds?.has(item.id)}")
    expect(source).toContain("onExpandedChange(item.id, !showDetails)")
  })

  test("keeps timeline groups free of the pierre provider wrapper", () => {
    const source = loadToolTimelineSource()

    expect(source).not.toContain("PierreDiffProvider")
  })

  test("renders shell details on the flat path without generic request or result sections", () => {
    const source = loadToolDetailsPanelSource()

    expect(source).toContain(
      "const renderer = toolRendererRegistry.resolve(item.toolName)"
    )
    expect(source).toContain(
      'const detailsPanelMode = renderer.detailsPanelMode ?? "default"'
    )
    expect(source).toContain('if (detailsPanelMode === "renderer-only-flat")')
    expect(source).toContain(
      '<ToolDetailSurface className="tool-timeline-detail-surface-flat">'
    )
    expect(source).not.toContain(
      'const detailsFirst = normalizedToolName === "Shell"'
    )
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
            toolName: "Edit",
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

  test("renders tool rows as expandable rows without a caret icon", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-shell-caretless-1",
            toolName: "Shell",
            arguments: {
              command: "cargo check",
              description: "Run workspace checks",
            },
            startedAtMs: 80,
            finishedAtMs: 99,
            succeeded: true,
            outputContent: "ok",
            details: {
              command: "cargo check",
              stdout: "ok",
            },
          },
          {
            id: "tool-edit-caretless-1",
            toolName: "Edit",
            arguments: {
              file_path: "apps/web/src/index.css",
            },
            startedAtMs: 99,
            finishedAtMs: 100,
            succeeded: true,
            outputContent: "@@\n-old\n+new",
            details: {
              added: 1,
              removed: 1,
              diff: "@@\n-old\n+new",
            },
          },
          {
            id: "tool-write-1",
            toolName: "Write",
            arguments: {
              file_path: "apps/web/src/index.css",
            },
            startedAtMs: 100,
            finishedAtMs: 220,
            succeeded: true,
            outputContent: "Wrote 28 bytes to apps/web/src/index.css",
            details: {
              lines: 1,
            },
          },
          {
            id: "tool-patch-1",
            toolName: "ApplyPatch",
            arguments: {
              patch:
                "*** Begin Patch\n*** Update File: apps/web/src/index.css\n@@\n-old\n+new\n*** End Patch",
            },
            startedAtMs: 221,
            finishedAtMs: 260,
            succeeded: true,
            outputContent:
              "*** Begin Patch\n*** Update File: apps/web/src/index.css\n@@\n-old\n+new\n*** End Patch",
            details: {
              lines_added: 1,
              lines_removed: 1,
            },
          },
        ]}
      />
    )

    expect(html).toContain('aria-expanded="false"')
    expect(html).not.toContain('data-slot="tool-row-caret"')
    expect(html).not.toContain('data-slot="tool-row-inline-details"')
  })

  test("renders ApplyPatch subtitle from renderer instead of title fallback", () => {
    const html = renderWithTheme(
      <ToolGroup
        items={[
          {
            id: "tool-patch-subtitle-1",
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
            startedAtMs: 221,
            finishedAtMs: 260,
            succeeded: true,
            outputContent: "",
            details: {
              lines_added: 2,
              lines_removed: 1,
            },
          },
        ]}
      />
    )

    expect(html).toContain("ApplyPatch")
    expect(html).toContain("old.txt → nested/new.txt +1 files")
  })

  test("keeps all tool rows on the caretless expandable path", () => {
    const source = loadToolRowPolicySource()

    expect(source).toContain("const INLINE_DETAIL_TOOLS = new Set<string>()")
    expect(source).toContain("function shouldShowToolRowCaret()")
    expect(source).toContain("return false")
  })

  test("does not render caret containers for tool rows or context groups", () => {
    const source = loadToolTimelineSource()

    expect(source).not.toContain('data-slot="tool-row-caret"')
    expect(source).not.toContain('data-slot="context-group-caret"')
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
