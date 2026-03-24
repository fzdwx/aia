import { renderToStaticMarkup } from "react-dom/server"
import { readFileSync } from "node:fs"
import type { ReactElement } from "react"
import { describe, expect, test } from "vite-plus/test"
import { ThemeProvider } from "@/components/theme-provider"

import {
  CompressionNotice,
  MemoizedStreamingView,
  MemoizedTurnView,
  SessionHydratingIndicator,
  StatusIndicator,
  UserMessageBlock,
} from "./message-sections"
import { formatDurationMs } from "./tool-timeline-helpers"

function renderWithTheme(element: ReactElement) {
  return renderToStaticMarkup(
    <ThemeProvider defaultTheme="dark">{element}</ThemeProvider>
  )
}

const CHAT_MESSAGES_TSX = new URL(
  "../../components/chat-messages.tsx",
  import.meta.url
)
const MESSAGE_SECTIONS_TSX = new URL("./message-sections.tsx", import.meta.url)

function loadChatMessagesSource() {
  return readFileSync(CHAT_MESSAGES_TSX, "utf8").replace(/\s+/g, " ")
}

function loadMessageSectionsSource() {
  return readFileSync(MESSAGE_SECTIONS_TSX, "utf8").replace(/\s+/g, " ")
}

describe("chat message status surfaces", () => {
  test("merges only consecutive context exploration tools", () => {
    const html = renderWithTheme(
      <MemoizedTurnView
        turn={{
          turn_id: "turn-tools-context",
          started_at_ms: 100,
          finished_at_ms: 200,
          source_entry_ids: [1],
          user_message: "Question",
          blocks: [
            {
              kind: "tool_invocation",
              invocation: {
                call: {
                  invocation_id: "tool-read",
                  tool_name: "Read",
                  arguments: { file_path: "docs/status.md" },
                },
                started_at_ms: 110,
                finished_at_ms: 120,
                outcome: {
                  status: "succeeded",
                  result: {
                    invocation_id: "tool-read",
                    tool_name: "Read",
                    content: "status",
                    details: {},
                  },
                },
              },
            },
            {
              kind: "tool_invocation",
              invocation: {
                call: {
                  invocation_id: "tool-grep",
                  tool_name: "Grep",
                  arguments: { pattern: "phase", path: "docs" },
                },
                started_at_ms: 121,
                finished_at_ms: 130,
                outcome: {
                  status: "succeeded",
                  result: {
                    invocation_id: "tool-grep",
                    tool_name: "Grep",
                    content: "phase",
                    details: {},
                  },
                },
              },
            },
          ],
          assistant_message: null,
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        }}
      />
    )

    expect(html.match(/Explored/g)).toHaveLength(1)
    expect(html).toContain("1 read")
    expect(html).toContain("1 search")
  })

  test("breaks context groups on non-context tools and non-tool blocks", () => {
    const html = renderWithTheme(
      <MemoizedTurnView
        turn={{
          turn_id: "turn-tools-breaks",
          started_at_ms: 100,
          finished_at_ms: 260,
          source_entry_ids: [1],
          user_message: "Question",
          blocks: [
            {
              kind: "tool_invocation",
              invocation: {
                call: {
                  invocation_id: "tool-read-1",
                  tool_name: "Read",
                  arguments: { file_path: "docs/status.md" },
                },
                started_at_ms: 110,
                finished_at_ms: 120,
                outcome: {
                  status: "succeeded",
                  result: {
                    invocation_id: "tool-read-1",
                    tool_name: "Read",
                    content: "status",
                    details: {},
                  },
                },
              },
            },
            {
              kind: "tool_invocation",
              invocation: {
                call: {
                  invocation_id: "tool-shell",
                  tool_name: "Shell",
                  arguments: { command: "cargo check" },
                },
                started_at_ms: 121,
                finished_at_ms: 140,
                outcome: {
                  status: "succeeded",
                  result: {
                    invocation_id: "tool-shell",
                    tool_name: "Shell",
                    content: "ok",
                    details: {},
                  },
                },
              },
            },
            {
              kind: "tool_invocation",
              invocation: {
                call: {
                  invocation_id: "tool-glob",
                  tool_name: "Glob",
                  arguments: { pattern: "src/**/*.ts" },
                },
                started_at_ms: 141,
                finished_at_ms: 160,
                outcome: {
                  status: "succeeded",
                  result: {
                    invocation_id: "tool-glob",
                    tool_name: "Glob",
                    content: "matches",
                    details: {},
                  },
                },
              },
            },
            { kind: "assistant", content: "Done searching." },
            {
              kind: "tool_invocation",
              invocation: {
                call: {
                  invocation_id: "tool-list",
                  tool_name: "list",
                  arguments: { path: "apps/web/src/features/chat" },
                },
                started_at_ms: 200,
                finished_at_ms: 220,
                outcome: {
                  status: "succeeded",
                  result: {
                    invocation_id: "tool-list",
                    tool_name: "list",
                    content: "entries",
                    details: {},
                  },
                },
              },
            },
          ],
          assistant_message: "Done searching.",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        }}
      />
    )

    expect(html.match(/Explored/g)).toHaveLength(3)
    expect(html.match(/1 search/g)).toHaveLength(1)
    expect(html).toContain("1 read")
    expect(html).toContain("1 list")
    expect(html).toContain("Shell")
    expect(html).toContain("Done searching.")
  })

  test("renders the streaming status as a polite live region", () => {
    const html = renderToStaticMarkup(<StatusIndicator status="working" />)

    expect(html).toContain('role="status"')
    expect(html).toContain('aria-live="polite"')
    expect(html).toContain("Working")
  })

  test("removes motion-heavy hydration decoration when reduced motion is preferred", () => {
    const html = renderToStaticMarkup(
      <SessionHydratingIndicator reducedMotion />
    )

    expect(html).toContain('role="status"')
    expect(html).toContain("Loading session")
    expect(html).not.toContain("animate-pulse")
    expect(html).toContain("text-caption")
  })

  test("uses shared auxiliary scale for thinking toggle and expanded content", () => {
    const html = renderWithTheme(
      <MemoizedStreamingView
        streaming={{
          userMessage: "Question",
          status: "thinking",
          blocks: [{ type: "thinking", content: "first\nsecond" }],
        }}
      />
    )

    expect(html).toContain(
      "text-body-sm leading-body-sm flex w-full items-baseline"
    )
    expect(html).toContain("text-body-sm leading-body-sm")
    expect(html).not.toContain("text-[13px]")
  })

  test("adds a touch more vertical breathing room to reasoning blocks", () => {
    const source = loadMessageSectionsSource()

    expect(source).toContain("className={`${MESSAGE_READING_MEASURE} py-1`}")
    expect(source).toContain(
      'className="text-body-sm leading-body-sm mt-2.5 border-l-2 border-border/30 pl-3"'
    )
  })

  test("uses a roomier stack gap between text and auxiliary blocks", () => {
    const source = loadMessageSectionsSource()

    expect(source).toContain(
      'className="group/turn flex w-full flex-col gap-4"'
    )
    expect(source).toContain('className="flex w-full flex-col gap-4"')
  })

  test("uses shared auxiliary scale for turn meta and compression notice", () => {
    const turnHtml = renderWithTheme(
      <MemoizedTurnView
        turn={{
          turn_id: "turn-meta",
          started_at_ms: 100,
          finished_at_ms: 300,
          source_entry_ids: [1],
          user_message: "Question",
          blocks: [{ kind: "assistant", content: "Answer" }],
          assistant_message: "Answer",
          thinking: null,
          tool_invocations: [],
          usage: {
            input_tokens: 120,
            output_tokens: 80,
            total_tokens: 200,
            cached_tokens: 30,
          },
          failure_message: "failed",
          outcome: "failed",
        }}
      />
    )
    const compressionHtml = renderToStaticMarkup(
      <CompressionNotice summary="trimmed context" />
    )

    expect(turnHtml).toContain("text-caption mt-2 flex items-center")
    expect(turnHtml).toContain("text-caption font-normal")
    expect(turnHtml).toContain("text-label rounded-full")
    expect(compressionHtml).toContain("text-caption")
    expect(compressionHtml).toContain("workspace-section-label")
  })

  test("does not add extra tool-group spacing wrappers in message sections", () => {
    const source = loadMessageSectionsSource()

    expect(source).not.toContain("toolGroupSpacingClass")
    expect(source).not.toContain("style={toolGroupSpacingClass")
  })

  test("uses shared auxiliary scale for history hint in message list", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain("Scroll up for older messages")
    expect(source).toContain("text-center text-xs text-muted-foreground/80")
  })

  test("drops session scroll restoration refs while keeping bottom-follow entrypoint", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain("scrollToBottom()")
    expect(source).toContain("if (turns.length === 0 && !streamingTurn) return")
    expect(source).not.toContain("historyTriggerRef")
    expect(source).not.toContain("bottomAnchorRef")
    expect(source).not.toContain("previousScrollHeightRef")
    expect(source).not.toContain("sessionBottomLockRef")
  })

  test("only pages older history from upward scrolling instead of top visibility", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain("shouldLoadOlderTurnsOnScroll")
    expect(source).toContain("userScrolledUp")
    expect(source).not.toContain("IntersectionObserver")
  })

  test("keeps streaming tool durations on an interval ticker", () => {
    const source = readFileSync(
      new URL("./tool-timeline.tsx", import.meta.url),
      "utf8"
    ).replace(/\s+/g, " ")

    expect(source).toContain("const ACTIVE_DURATION_TICK_MS = 100")
    expect(source).toContain("window.setInterval")
    expect(source).toContain("useDurationTicker(item.finishedAtMs == null)")
    expect(source).toContain("useDurationTicker(active.length > 0)")
  })

  test("formats live tool durations in smooth seconds", () => {
    const now = Date.now()
    const live = formatDurationMs(now - 340, undefined, { live: true })
    const finished = formatDurationMs(now - 340, now)

    expect(live).toBe("0.3 s")
    expect(finished).toBe("340 ms")
  })

  test("uses auxiliary-alert tier for failure and cancelled blocks", () => {
    const html = renderWithTheme(
      <MemoizedTurnView
        turn={{
          turn_id: "turn-alert",
          started_at_ms: 100,
          finished_at_ms: 300,
          source_entry_ids: [1],
          user_message: "Question",
          blocks: [
            { kind: "failure", message: "Tool execution failed" },
            { kind: "cancelled", message: "Turn was cancelled" },
          ],
          assistant_message: null,
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: "Tool execution failed",
          outcome: "failed",
        }}
      />
    )

    expect(html).toContain(
      "text-caption rounded-lg border border-destructive/30"
    )
    expect(html).toContain("text-caption rounded-lg border border-border/40")
    expect(html).not.toContain("text-[13px]")
  })

  test("uses auxiliary-alert tier for in-flow chat error banner", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain(
      "text-xs leading-relaxed font-medium text-destructive"
    )
  })

  test("keeps user messages on the single-column width", () => {
    const html = renderWithTheme(
      <UserMessageBlock content={"A compact user message"} />
    )

    expect(html).toContain('data-component="user-message"')
    expect(html).toContain("justify-start")
    expect(html).not.toContain("justify-end")
    expect(html).toContain("w-full max-w-full")
    expect(html).toContain("text-body-sm")
    expect(html).toContain("Copy message")
  })

  test("uses message bodies instead of role heading rows", () => {
    const html = renderWithTheme(
      <MemoizedTurnView
        turn={{
          turn_id: "turn-1",
          started_at_ms: 100,
          finished_at_ms: 200,
          source_entry_ids: [1],
          user_message: "Question",
          blocks: [{ kind: "assistant", content: "# Title\n\nAnswer" }],
          assistant_message: "# Title\n\nAnswer",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        }}
      />
    )

    expect(html).toContain('data-component="assistant-message"')
    expect(html).toContain('data-component="text-part"')
    expect(html).toContain('data-slot="user-message-copy-wrapper"')
    expect(html).toContain('data-slot="text-part-copy-wrapper"')
    expect(html).not.toContain("workspace-section-label text-foreground/70")
  })

  test("keeps assistant markdown on the single-column width in completed turns", () => {
    const html = renderWithTheme(
      <MemoizedTurnView
        turn={{
          turn_id: "turn-1",
          started_at_ms: 100,
          finished_at_ms: 200,
          source_entry_ids: [1],
          user_message: "Question",
          blocks: [{ kind: "assistant", content: "# Title\n\nAnswer" }],
          assistant_message: "# Title\n\nAnswer",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        }}
      />
    )

    expect(html).toContain('data-component="text-part"')
    expect(html).toContain('data-slot="text-part-body"')
    expect(html).toContain("w-full")
    expect(html).toContain("Copy response")
  })

  test("keeps assistant markdown on the single-column width while streaming", () => {
    const html = renderWithTheme(
      <MemoizedStreamingView
        streaming={{
          userMessage: "Question",
          status: "generating",
          blocks: [{ type: "text", content: "Streaming answer" }],
        }}
      />
    )

    expect(html).toContain('data-component="assistant-message"')
    expect(html).toContain("Copy response")
    expect(html).toContain("w-full")
  })
})
