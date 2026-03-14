import { afterEach, beforeEach, describe, test } from "node:test"
import assert from "node:assert/strict"

import { useChatStore } from "./chat-store"
import type { SseEvent } from "@/lib/types"

type FetchMock = typeof fetch

const initialState = {
  turns: [],
  streamingTurn: null,
  chatState: "idle" as const,
  provider: null,
  providerList: [],
  error: null,
  view: "chat" as const,
  contextPressure: null,
  _pendingPrompt: null,
}

describe("chat store submitTurn", () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    useChatStore.setState(initialState)
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  test("shows user message immediately after submit", () => {
    globalThis.fetch = (async () =>
      new Response(null, { status: 202 })) as FetchMock

    useChatStore.getState().submitTurn("hello world")

    const state = useChatStore.getState()
    assert.equal(state.chatState, "active")
    assert.deepEqual(state.streamingTurn, {
      userMessage: "hello world",
      status: "waiting",
      blocks: [],
    })
    assert.equal(state.error, null)
  })

  test("waiting status does not wipe optimistic streaming blocks", () => {
    useChatStore.setState({
      chatState: "active",
      _pendingPrompt: "hello world",
      streamingTurn: {
        userMessage: "hello world",
        status: "thinking",
        blocks: [{ type: "text", content: "partial answer" }],
      },
    })

    const waitingEvent: SseEvent = {
      type: "status",
      data: { status: "waiting" },
    }

    useChatStore.getState().handleSseEvent(waitingEvent)

    const state = useChatStore.getState()
    assert.deepEqual(state.streamingTurn, {
      userMessage: "hello world",
      status: "waiting",
      blocks: [{ type: "text", content: "partial answer" }],
    })
  })

  test("ignores duplicate global error after failed turn is already appended", () => {
    useChatStore.setState({
      turns: [
        {
          turn_id: "turn-1",
          started_at_ms: 1,
          finished_at_ms: 2,
          source_entry_ids: [1, 2],
          user_message: "what time is it",
          blocks: [
            {
              kind: "failure",
              message: "模型执行失败：请求失败：502 Bad Gateway",
            },
          ],
          assistant_message: null,
          thinking: null,
          tool_invocations: [],
          failure_message: "模型执行失败：请求失败：502 Bad Gateway",
        },
      ],
      streamingTurn: null,
      chatState: "idle",
      error: null,
    })

    const errorEvent: SseEvent = {
      type: "error",
      data: { message: "模型执行失败：请求失败：502 Bad Gateway" },
    }

    useChatStore.getState().handleSseEvent(errorEvent)

    assert.equal(useChatStore.getState().error, null)
  })
})
