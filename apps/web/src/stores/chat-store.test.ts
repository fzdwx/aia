import { afterEach, beforeEach, describe, test } from "node:test"
import assert from "node:assert/strict"

import { useChatStore } from "./chat-store"
import type { SseEvent } from "@/lib/types"

type FetchMock = typeof fetch

const initialState = {
  activeSessionId: "session-1",
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
      data: { session_id: "session-1", status: "waiting" },
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
          usage: null,
          failure_message: "模型执行失败：请求失败：502 Bad Gateway",
          outcome: "failed",
        },
      ],
      streamingTurn: null,
      chatState: "idle",
      error: null,
    })

    const errorEvent: SseEvent = {
      type: "error",
      data: {
        session_id: "session-1",
        message: "模型执行失败：请求失败：502 Bad Gateway",
      },
    }

    useChatStore.getState().handleSseEvent(errorEvent)

    assert.equal(useChatStore.getState().error, null)
  })

  test("creates tool block from started event with full arguments", () => {
    useChatStore.setState({
      chatState: "active",
      streamingTurn: {
        userMessage: "read AGENTS",
        status: "thinking",
        blocks: [],
      },
    })

    const startedEvent: SseEvent = {
      type: "stream",
      data: {
        session_id: "session-1",
        kind: "tool_call_started",
        invocation_id: "functions.read:27",
        tool_name: "functions.read",
        arguments: {
          file_path: "/home/like/projects/aia/AGENTS.md",
        },
      },
    }

    useChatStore.getState().handleSseEvent(startedEvent)

    const state = useChatStore.getState().streamingTurn
    assert.equal(state?.blocks.length, 1)
    assert.equal(state?.blocks[0]?.type, "tool")
    if (state?.blocks[0]?.type !== "tool") {
      throw new Error("expected tool block")
    }
    assert.equal(state.blocks[0].tool.invocationId, "functions.read:27")
    assert.equal(state.blocks[0].tool.toolName, "functions.read")
    assert.deepEqual(state.blocks[0].tool.arguments, {
      file_path: "/home/like/projects/aia/AGENTS.md",
    })
    assert.equal(typeof state.blocks[0].tool.startedAtMs, "number")
  })

  test("keeps partial streaming content visible after cancelled error", () => {
    useChatStore.setState({
      chatState: "active",
      streamingTurn: {
        userMessage: "hello world",
        status: "generating",
        blocks: [
          { type: "thinking", content: "先分析" },
          { type: "text", content: "部分回答" },
        ],
      },
      error: null,
    })

    const errorEvent: SseEvent = {
      type: "error",
      data: {
        session_id: "session-1",
        message: "本轮已取消",
      },
    }

    useChatStore.getState().handleSseEvent(errorEvent)

    const state = useChatStore.getState()
    assert.equal(state.chatState, "idle")
    assert.equal(state.error, null)
    assert.deepEqual(state.streamingTurn, {
      userMessage: "hello world",
      status: "cancelled",
      blocks: [
        { type: "thinking", content: "先分析" },
        { type: "text", content: "部分回答" },
      ],
    })
  })

  test("turn_completed with cancelled outcome replaces streaming turn with preserved history", () => {
    useChatStore.setState({
      chatState: "active",
      streamingTurn: {
        userMessage: "hello world",
        status: "cancelled",
        blocks: [
          { type: "thinking", content: "先分析" },
          { type: "text", content: "部分回答" },
        ],
      },
      turns: [],
      error: null,
    })

    const completedEvent: SseEvent = {
      type: "turn_completed",
      data: {
        session_id: "session-1",
        turn_id: "turn-cancelled-1",
        started_at_ms: 1,
        finished_at_ms: 2,
        source_entry_ids: [1, 2, 3],
        user_message: "hello world",
        blocks: [
          { kind: "thinking", content: "先分析" },
          { kind: "assistant", content: "部分回答" },
          { kind: "failure", message: "本轮已取消" },
        ],
        assistant_message: "部分回答",
        thinking: "先分析",
        tool_invocations: [],
        usage: null,
        failure_message: "本轮已取消",
        outcome: "cancelled",
      },
    }

    useChatStore.getState().handleSseEvent(completedEvent)

    const state = useChatStore.getState()
    assert.equal(state.chatState, "idle")
    assert.equal(state.streamingTurn, null)
    assert.equal(state.turns.length, 1)
    assert.equal(state.turns[0]?.outcome, "cancelled")
    assert.equal(state.turns[0]?.assistant_message, "部分回答")
  })
})
