import { afterEach, beforeEach, describe, test } from "node:test"
import assert from "node:assert/strict"

import { useChatStore } from "./chat-store"
import type { SseEvent } from "@/lib/types"

type FetchMock = typeof fetch

const initialState = {
  activeSessionId: "session-1",
  sessionHydrating: false,
  turns: [],
  historyHasMore: false,
  historyNextBeforeTurnId: null,
  historyLoadingMore: false,
  streamingTurn: null,
  chatState: "idle" as const,
  provider: null,
  providerList: [],
  error: null,
  view: "chat" as const,
  contextPressure: null,
  lastCompression: null,
  _pendingPrompt: null,
  _sessionSnapshots: {},
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

  test("stores context compression notice for active session", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      lastCompression: null,
    })

    const compressionEvent: SseEvent = {
      type: "context_compressed",
      data: {
        session_id: "session-1",
        summary: "摘要：已压缩旧历史，保留当前任务目标。",
      },
    }

    useChatStore.getState().handleSseEvent(compressionEvent)

    const state = useChatStore.getState()
    assert.deepEqual(state.lastCompression, compressionEvent.data)
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
      _sessionSnapshots: {
        "session-1": {
          turns: [],
          historyHasMore: false,
          historyNextBeforeTurnId: null,
          streamingTurn: {
            userMessage: "hello world",
            status: "cancelled",
            blocks: [
              { type: "thinking", content: "先分析" },
              { type: "text", content: "部分回答" },
            ],
          },
          chatState: "idle",
          contextPressure: null,
          lastCompression: null,
        },
      },
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
    assert.equal(state._sessionSnapshots["session-1"]?.turns.length, 1)
    assert.equal(state._sessionSnapshots["session-1"]?.streamingTurn, null)
  })

  test("switchSession keeps cached snapshot visible while hydrating next session", async () => {
    const originalFetchImpl = globalThis.fetch
    let resolveHistory: ((value: Response) => void) | null = null
    let resolveCurrentTurn: ((value: Response) => void) | null = null

    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url.includes("/api/session/info")) {
        return new Response(
          JSON.stringify({
            total_entries: 0,
            anchor_count: 0,
            entries_since_last_anchor: 0,
            last_input_tokens: null,
            context_limit: null,
            output_limit: null,
            pressure_ratio: 0.25,
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      if (url.includes("/api/session/history")) {
        return await new Promise<Response>((resolve) => {
          resolveHistory = resolve
        })
      }
      if (url.includes("/api/session/current-turn")) {
        return await new Promise<Response>((resolve) => {
          resolveCurrentTurn = resolve
        })
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      sessionHydrating: false,
      turns: [
        {
          turn_id: "turn-1",
          started_at_ms: 1,
          finished_at_ms: 2,
          source_entry_ids: [1],
          user_message: "old session question",
          blocks: [{ kind: "assistant", content: "old session answer" }],
          assistant_message: "old session answer",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        },
      ],
      _sessionSnapshots: {
        "session-1": {
          turns: [
            {
              turn_id: "turn-1",
              started_at_ms: 1,
              finished_at_ms: 2,
              source_entry_ids: [1],
              user_message: "old session question",
              blocks: [{ kind: "assistant", content: "old session answer" }],
              assistant_message: "old session answer",
              thinking: null,
              tool_invocations: [],
              usage: null,
              failure_message: null,
              outcome: "succeeded",
            },
          ],
          historyHasMore: false,
          historyNextBeforeTurnId: null,
          streamingTurn: null,
          chatState: "idle",
          contextPressure: null,
          lastCompression: null,
        },
        "session-2": {
          turns: [
            {
              turn_id: "turn-2-cached",
              started_at_ms: 10,
              finished_at_ms: 20,
              source_entry_ids: [2],
              user_message: "cached question",
              blocks: [{ kind: "assistant", content: "cached answer" }],
              assistant_message: "cached answer",
              thinking: null,
              tool_invocations: [],
              usage: null,
              failure_message: null,
              outcome: "succeeded",
            },
          ],
          historyHasMore: false,
          historyNextBeforeTurnId: null,
          streamingTurn: null,
          chatState: "idle",
          contextPressure: 0.1,
          lastCompression: null,
        },
      },
    })

    const switchPromise = useChatStore.getState().switchSession("session-2")

    const duringHydration = useChatStore.getState()
    assert.equal(duringHydration.activeSessionId, "session-2")
    assert.equal(duringHydration.sessionHydrating, true)
    assert.equal(duringHydration.turns[0]?.turn_id, "turn-2-cached")

    resolveHistory?.(
      new Response(
        JSON.stringify({
          turns: [
            {
              turn_id: "turn-2-live",
              started_at_ms: 11,
              finished_at_ms: 21,
              source_entry_ids: [3],
              user_message: "live question",
              blocks: [{ kind: "assistant", content: "live answer" }],
              assistant_message: "live answer",
              thinking: null,
              tool_invocations: [],
              usage: null,
              failure_message: null,
              outcome: "succeeded",
            },
          ],
          has_more: false,
          next_before_turn_id: null,
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }
      )
    )
    resolveCurrentTurn?.(
      new Response("null", {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    )

    await switchPromise

    const hydrated = useChatStore.getState()
    assert.equal(hydrated.sessionHydrating, false)
    assert.equal(hydrated.turns[0]?.turn_id, "turn-2-live")
    assert.equal(hydrated._sessionSnapshots["session-1"]?.turns[0]?.turn_id, "turn-1")

    globalThis.fetch = originalFetchImpl
  })
})
