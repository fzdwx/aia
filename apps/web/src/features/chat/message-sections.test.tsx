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

function renderWithTheme(element: ReactElement) {
  return renderToStaticMarkup(
    <ThemeProvider defaultTheme="dark">{element}</ThemeProvider>
  )
}

const CHAT_MESSAGES_TSX = new URL(
  "../../components/chat-messages.tsx",
  import.meta.url
)

function loadChatMessagesSource() {
  return readFileSync(CHAT_MESSAGES_TSX, "utf8").replace(/\s+/g, " ")
}

describe("chat message status surfaces", () => {
  test("renders the streaming status as a polite live region", () => {
    const html = renderToStaticMarkup(<StatusIndicator status="working" />)

    expect(html).toContain('role="status"')
    expect(html).toContain('aria-live="polite"')
    expect(html).toContain("Working")
    expect(html).toContain("text-sm font-medium")
  })

  test("removes motion-heavy hydration decoration when reduced motion is preferred", () => {
    const html = renderToStaticMarkup(
      <SessionHydratingIndicator reducedMotion />
    )

    expect(html).toContain('role="status"')
    expect(html).toContain("Loading session")
    expect(html).not.toContain("animate-pulse")
    expect(html).toContain("text-xs text-muted-foreground/80")
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

    expect(html).toContain("text-xs text-muted-foreground")
    expect(html).toContain("text-xs leading-relaxed text-muted-foreground/80")
    expect(html).not.toContain("text-[13px]")
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

    expect(turnHtml).toContain("text-xs text-muted-foreground/55")
    expect(turnHtml).toContain("text-xs font-normal")
    expect(turnHtml).toContain("text-[0.6875rem] font-medium")
    expect(compressionHtml).toContain("text-xs text-muted-foreground")
    expect(compressionHtml).toContain("text-[0.6875rem] font-semibold")
  })

  test("uses shared auxiliary scale for history hint in message list", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain("Scroll up for older messages")
    expect(source).toContain("text-center text-xs text-muted-foreground/80")
  })

  test("keeps direct bottom-anchor scrolling for session recovery", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain("bottomAnchorRef")
    expect(source).toContain("scrollIntoView")
    expect(source).not.toContain("MutationObserver")
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
      "text-xs leading-relaxed font-medium text-destructive"
    )
    expect(html).toContain(
      "text-xs leading-relaxed font-medium text-muted-foreground"
    )
    expect(html).not.toContain("text-[13px]")
  })

  test("uses auxiliary-alert tier for in-flow chat error banner", () => {
    const source = loadChatMessagesSource()

    expect(source).toContain(
      "text-xs leading-relaxed font-medium text-destructive"
    )
  })

  test("keeps user markdown at a consistent reading measure", () => {
    const html = renderWithTheme(
      <UserMessageBlock content={"A compact user message"} />
    )

    expect(html).toContain("max-w-[66ch]")
    expect(html).toContain("text-sm")
  })

  test("keeps assistant markdown constrained in completed turns", () => {
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

    expect(html).toContain("max-w-[66ch]")
  })

  test("keeps assistant markdown constrained while streaming", () => {
    const html = renderWithTheme(
      <MemoizedStreamingView
        streaming={{
          userMessage: "Question",
          status: "generating",
          blocks: [{ type: "text", content: "Streaming answer" }],
        }}
      />
    )

    expect(html).toContain("max-w-[66ch]")
  })
})
