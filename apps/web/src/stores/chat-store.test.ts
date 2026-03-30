import { afterEach, beforeEach, describe, expect, test } from "vite-plus/test"
import assert from "node:assert/strict"

import { __setIdleSchedulerForTests, useChatStore } from "./chat-store"
import { usePendingQuestionStore } from "./pending-question-store"
import { useProviderRegistryStore } from "./provider-registry-store"
import { switchActiveSessionModel } from "./session-settings-runtime"
import { useSessionSettingsStore } from "./session-settings-store"
import type { SseEvent } from "@/lib/types"

type FetchMock = typeof fetch
type ResponseResolver = (value: Response) => void

function requireResolver<T>(
  resolver: T | null | undefined,
  message: string
): T {
  if (resolver == null) {
    throw new Error(message)
  }
  return resolver
}

const initialState = {
  sessions: [],
  sessionTitleAnimations: {},
  activeSessionId: "session-1",
  sessionHydrating: false,
  turns: [],
  historyHasMore: false,
  historyNextBeforeTurnId: null,
  historyLoadingMore: false,
  streamingTurn: null,
  chatState: "idle" as const,
  error: null,
  contextPressure: null,
  lastCompression: null,
  _pendingPrompt: null,
  _sessionSnapshots: {},
}

describe("chat store submitTurn", () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    useChatStore.setState(initialState)
    usePendingQuestionStore.setState({
      pendingQuestion: null,
      hydrating: false,
      submitting: false,
      error: null,
      hydrateForSession: async () => {},
      clear: () => {},
      submitResult: async () => {},
      cancel: async () => {},
    })
    useProviderRegistryStore.setState({ providerList: [] })
    __setIdleSchedulerForTests({
      schedule: (callback) => {
        callback()
        return 0
      },
      cancel: () => {},
    })
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
    __setIdleSchedulerForTests(null)
  })

  test("shows user message immediately after submit", () => {
    globalThis.fetch = (async () =>
      new Response(null, { status: 202 })) as FetchMock

    useChatStore.getState().submitTurn("hello world")

    const state = useChatStore.getState()
    expect(state.chatState).toBe("active")
    assert.deepEqual(state.streamingTurn, {
      userMessages: ["hello world"],
      status: "waiting",
      blocks: [],
    })
    expect(state.error).toBe(null)
  })

  test("submit failure hydrates pending question for active session", async () => {
    let hydratedSessionId: string | null = null
    usePendingQuestionStore.setState({
      hydrateForSession: async (sessionId: string) => {
        hydratedSessionId = sessionId
      },
    })

    globalThis.fetch = (async () =>
      new Response(null, { status: 400 })) as FetchMock

    useChatStore.getState().submitTurn("hello world")
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(hydratedSessionId).toBe("session-1")
  })

  test("turn completed hydrates pending question for active session", () => {
    let hydratedSessionId: string | null = null
    usePendingQuestionStore.setState({
      hydrateForSession: async (sessionId: string) => {
        hydratedSessionId = sessionId
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "turn_completed",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        started_at_ms: 1,
        finished_at_ms: 2,
        source_entry_ids: [1, 2],
        user_messages: ["hello"],
        blocks: [],
        assistant_message: null,
        thinking: null,
        tool_invocations: [],
        usage: null,
        failure_message: null,
        outcome: "succeeded",
      },
    })

    expect(hydratedSessionId).toBe("session-1")
  })

  test("sync required hydrates pending question for active session", () => {
    let hydratedSessionId: string | null = null
    usePendingQuestionStore.setState({
      hydrateForSession: async (sessionId: string) => {
        hydratedSessionId = sessionId
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "sync_required",
      data: { reason: "lagged", skipped_messages: 3 },
    })

    expect(hydratedSessionId).toBe("session-1")
  })

  test("waiting status does not wipe optimistic streaming blocks", () => {
    useChatStore.setState({
      chatState: "active",
      _pendingPrompt: "hello world",
      streamingTurn: {
        userMessages: ["hello world"],
        status: "thinking",
        blocks: [{ type: "text", content: "partial answer" }],
      },
    })

    const waitingEvent: SseEvent = {
      type: "status",
      data: { session_id: "session-1", turn_id: "turn-1", status: "waiting" },
    }

    useChatStore.getState().handleSseEvent(waitingEvent)

    const state = useChatStore.getState()
    assert.deepEqual(state.streamingTurn, {
      userMessages: ["hello world"],
      status: "waiting",
      blocks: [{ type: "text", content: "partial answer" }],
    })
  })

  test("waiting_for_question status hydrates pending question for active session", () => {
    let hydratedSessionId: string | null = null
    usePendingQuestionStore.setState({
      hydrateForSession: async (sessionId: string) => {
        hydratedSessionId = sessionId
      },
    })

    useChatStore.setState({
      activeSessionId: "session-1",
      chatState: "active",
      streamingTurn: {
        userMessages: ["hello world"],
        status: "working",
        blocks: [],
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "status",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        status: "waiting_for_question",
      },
    })

    expect(hydratedSessionId).toBe("session-1")
    assert.deepEqual(useChatStore.getState().streamingTurn, {
      userMessages: ["hello world"],
      status: "waiting_for_question",
      blocks: [],
    })
  })

  test("finishing status updates streaming turn immediately after stream done", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      chatState: "active",
      streamingTurn: {
        userMessages: ["hello world"],
        status: "generating",
        blocks: [{ type: "text", content: "partial answer" }],
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "status",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        status: "finishing",
      },
    })

    const state = useChatStore.getState()
    expect(state.chatState).toBe("active")
    assert.deepEqual(state.streamingTurn, {
      userMessages: ["hello world"],
      status: "finishing",
      blocks: [{ type: "text", content: "partial answer" }],
    })
  })

  test("current_turn_started renders external inbound message immediately", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      streamingTurn: null,
      chatState: "idle",
    })

    useChatStore.getState().handleSseEvent({
      type: "current_turn_started",
      data: {
        session_id: "session-1",
        started_at_ms: 30,
        user_messages: ["飞书里来的问题"],
        status: "waiting",
        blocks: [],
      },
    })

    const state = useChatStore.getState()
    assert.deepEqual(state.streamingTurn, {
      userMessages: ["飞书里来的问题"],
      status: "waiting",
      blocks: [],
    })
    expect(state.chatState).toBe("active")
  })

  test("waiting status without pending prompt recovers current turn from server", async () => {
    const originalFetchImpl = globalThis.fetch
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url.includes("/api/session/current-turn")) {
        return new Response(
          JSON.stringify({
            started_at_ms: 30,
            user_messages: ["飞书外部消息"],
            status: "working",
            blocks: [{ kind: "text", content: "已恢复中的输出" }],
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      streamingTurn: null,
      _pendingPrompt: null,
      chatState: "idle",
    })

    useChatStore.getState().handleSseEvent({
      type: "status",
      data: { session_id: "session-1", turn_id: "turn-1", status: "waiting" },
    })

    await new Promise((resolve) => setTimeout(resolve, 0))

    assert.deepEqual(useChatStore.getState().streamingTurn, {
      userMessages: ["飞书外部消息"],
      status: "working",
      blocks: [{ type: "text", content: "已恢复中的输出" }],
    })

    globalThis.fetch = originalFetchImpl
  })

  test("recovering a current turn waiting for question hydrates pending question", async () => {
    const originalFetchImpl = globalThis.fetch
    let hydratedSessionId: string | null = null
    usePendingQuestionStore.setState({
      hydrateForSession: async (sessionId: string) => {
        hydratedSessionId = sessionId
      },
    })
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url.includes("/api/session/current-turn")) {
        return new Response(
          JSON.stringify({
            started_at_ms: 30,
            user_messages: ["工具在等回答"],
            status: "waiting_for_question",
            blocks: [],
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      streamingTurn: null,
      _pendingPrompt: null,
      chatState: "idle",
    })

    useChatStore.getState().handleSseEvent({
      type: "status",
      data: { session_id: "session-1", turn_id: "turn-1", status: "waiting" },
    })

    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(hydratedSessionId).toBe("session-1")
    assert.deepEqual(useChatStore.getState().streamingTurn, {
      userMessages: ["工具在等回答"],
      status: "waiting_for_question",
      blocks: [],
    })

    globalThis.fetch = originalFetchImpl
  })

  test("stream event without local snapshot recovers current turn", async () => {
    const originalFetchImpl = globalThis.fetch
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url.includes("/api/session/current-turn")) {
        return new Response(
          JSON.stringify({
            started_at_ms: 42,
            user_messages: ["恢复中的问题"],
            status: "generating",
            blocks: [{ kind: "thinking", content: "先恢复上下文" }],
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      streamingTurn: null,
      chatState: "idle",
    })

    useChatStore.getState().handleSseEvent({
      type: "stream",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        kind: "text_delta",
        text: "后续增量",
      },
    })

    await new Promise((resolve) => setTimeout(resolve, 0))

    assert.deepEqual(useChatStore.getState().streamingTurn, {
      userMessages: ["恢复中的问题"],
      status: "generating",
      blocks: [{ type: "thinking", content: "先恢复上下文" }],
    })

    globalThis.fetch = originalFetchImpl
  })

  test("ignores duplicate global error after failed turn is already appended", () => {
    useChatStore.setState({
      turns: [
        {
          turn_id: "turn-1",
          started_at_ms: 1,
          finished_at_ms: 2,
          source_entry_ids: [1, 2],
          user_messages: ["what time is it"],
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

    expect(useChatStore.getState().error).toBe(null)
  })

  test("creates tool block from started event with full arguments", () => {
    useChatStore.setState({
      chatState: "active",
      streamingTurn: {
        userMessages: ["read AGENTS"],
        status: "thinking",
        blocks: [],
      },
    })

    const startedEvent: SseEvent = {
      type: "stream",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        kind: "tool_call_started",
        invocation_id: "Read:27",
        tool_name: "Read",
        arguments: {
          file_path: "/home/like/projects/aia/AGENTS.md",
        },
        started_at_ms: 123,
      },
    }

    useChatStore.getState().handleSseEvent(startedEvent)

    const state = useChatStore.getState().streamingTurn
    expect(state?.blocks.length).toBe(1)
    expect(state?.blocks[0]?.type).toBe("tool")
    if (state?.blocks[0]?.type !== "tool") {
      throw new Error("expected tool block")
    }
    expect(state.blocks[0].tool.invocationId).toBe("Read:27")
    expect(state.blocks[0].tool.toolName).toBe("Read")
    assert.deepEqual(state.blocks[0].tool.arguments, {
      file_path: "/home/like/projects/aia/AGENTS.md",
    })
    expect(state.blocks[0].tool.startedAtMs).toBe(123)
  })

  test("keeps tool duration timestamps aligned with backend stream events", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      chatState: "active",
      streamingTurn: {
        userMessages: ["search docs"],
        status: "working",
        blocks: [],
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "stream",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        kind: "tool_call_detected",
        invocation_id: "WebSearch:1",
        tool_name: "WebSearch",
        arguments: { q: "agent runtime timestamps" },
        detected_at_ms: 1000,
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "stream",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        kind: "tool_call_started",
        invocation_id: "WebSearch:1",
        tool_name: "WebSearch",
        arguments: { q: "agent runtime timestamps" },
        started_at_ms: 2200,
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "stream",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        kind: "tool_call_completed",
        invocation_id: "WebSearch:1",
        tool_name: "WebSearch",
        content: "done",
        failed: false,
        finished_at_ms: 14350,
      },
    })

    const state = useChatStore.getState().streamingTurn
    expect(state?.blocks).toHaveLength(1)
    expect(state?.blocks[0]?.type).toBe("tool")
    if (state?.blocks[0]?.type !== "tool") {
      throw new Error("expected tool block")
    }

    expect(state.blocks[0].tool.detectedAtMs).toBe(1000)
    expect(state.blocks[0].tool.startedAtMs).toBe(2200)
    expect(state.blocks[0].tool.finishedAtMs).toBe(14350)
  })

  test("creates a runnable tool timestamp when output arrives before started event", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      chatState: "active",
      streamingTurn: {
        userMessages: ["run checks"],
        status: "working",
        blocks: [],
      },
    })

    const before = Date.now()
    useChatStore.getState().handleSseEvent({
      type: "stream",
      data: {
        session_id: "session-1",
        turn_id: "turn-1",
        kind: "tool_output_delta",
        invocation_id: "Shell:1",
        stream: "stdout",
        text: "running...",
      },
    })
    const after = Date.now()

    const state = useChatStore.getState().streamingTurn
    expect(state?.blocks).toHaveLength(1)
    expect(state?.blocks[0]?.type).toBe("tool")
    if (state?.blocks[0]?.type !== "tool") {
      throw new Error("expected tool block")
    }

    expect(state.blocks[0].tool.startedAtMs).toBeDefined()
    expect(state.blocks[0].tool.startedAtMs).toBeGreaterThanOrEqual(before)
    expect(state.blocks[0].tool.startedAtMs).toBeLessThanOrEqual(after)
  })

  test("keeps partial streaming content visible after cancelled error", () => {
    useChatStore.setState({
      chatState: "active",
      streamingTurn: {
        userMessages: ["hello world"],
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
    expect(state.chatState).toBe("idle")
    expect(state.error).toBe(null)
    assert.deepEqual(state.streamingTurn, {
      userMessages: ["hello world"],
      status: "cancelled",
      blocks: [
        { type: "thinking", content: "先分析" },
        { type: "text", content: "部分回答" },
      ],
    })
  })

  test("shows global SSE errors even when session_id is missing", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      chatState: "active",
      streamingTurn: {
        userMessages: ["hello world"],
        status: "working",
        blocks: [{ type: "text", content: "partial answer" }],
      },
      error: null,
    })

    useChatStore.getState().handleSseEvent({
      type: "error",
      data: {
        session_id: null,
        turn_id: null,
        message: "Concurrency limit exceeded for user, please retry later",
      },
    })

    const state = useChatStore.getState()
    expect(state.chatState).toBe("idle")
    expect(state.streamingTurn).toBe(null)
    expect(state.error).toBe(
      "Concurrency limit exceeded for user, please retry later"
    )
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
    expect(state.lastCompression).toEqual(compressionEvent.data)
  })

  test("sync_required 会主动重拉当前会话状态", async () => {
    const originalFetchImpl = globalThis.fetch

    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url.includes("/api/sessions")) {
        return new Response(
          JSON.stringify([
            {
              id: "session-1",
              title: "当前会话",
              title_source: "manual",
              auto_rename_policy: "enabled",
              created_at: "2026-03-17T00:00:00Z",
              updated_at: "2026-03-17T00:00:00Z",
              last_active_at: "2026-03-17T00:00:00Z",
              model: "gpt-4.1-mini",
            },
            {
              id: "session-2",
              title: "补拉回来的新会话",
              title_source: "manual",
              auto_rename_policy: "enabled",
              created_at: "2026-03-17T00:01:00Z",
              updated_at: "2026-03-17T00:01:00Z",
              last_active_at: "2026-03-17T00:01:00Z",
              model: "gpt-4.1-mini",
            },
          ]),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      if (url.includes("/api/session/info")) {
        return new Response(
          JSON.stringify({
            total_entries: 12,
            anchor_count: 1,
            entries_since_last_anchor: 4,
            last_input_tokens: 320,
            context_limit: 128000,
            output_limit: 4096,
            pressure_ratio: 0.7,
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      if (url.includes("/api/session/current-turn")) {
        return new Response(
          JSON.stringify({
            started_at_ms: 30,
            user_messages: ["继续执行"],
            status: "working",
            blocks: [{ kind: "text", content: "已恢复中的输出" }],
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      if (url.includes("/api/session/history")) {
        return new Response(
          JSON.stringify({
            turns: [
              {
                turn_id: "turn-resynced",
                started_at_ms: 10,
                finished_at_ms: 20,
                source_entry_ids: [1, 2],
                user_messages: ["上一轮问题"],
                blocks: [{ kind: "assistant", content: "上一轮答案" }],
                assistant_message: "上一轮答案",
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
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      sessions: [
        {
          id: "session-1",
          title: "当前会话",
          title_source: "manual",
          auto_rename_policy: "enabled",
          created_at: "2026-03-17T00:00:00Z",
          updated_at: "2026-03-17T00:00:00Z",
          last_active_at: "2026-03-17T00:00:00Z",
          model: "gpt-4.1-mini",
        },
      ],
      turns: [
        {
          turn_id: "turn-stale",
          started_at_ms: 1,
          finished_at_ms: 2,
          source_entry_ids: [9],
          user_messages: ["过期问题"],
          blocks: [{ kind: "assistant", content: "过期答案" }],
          assistant_message: "过期答案",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        },
      ],
      streamingTurn: {
        userMessages: ["旧中的执行"],
        status: "thinking",
        blocks: [{ type: "text", content: "旧中的输出" }],
      },
      _sessionSnapshots: {
        "session-1": {
          latestTurn: {
            turn_id: "turn-stale",
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: [9],
            user_messages: ["过期问题"],
            blocks: [{ kind: "assistant", content: "过期答案" }],
            assistant_message: "过期答案",
            thinking: null,
            tool_invocations: [],
            usage: null,
            failure_message: null,
            outcome: "succeeded",
          },
          streamingTurn: {
            userMessages: ["旧中的执行"],
            status: "thinking",
            blocks: [{ type: "text", content: "旧中的输出" }],
          },
          chatState: "active",
          contextPressure: 0.1,
          lastCompression: null,
          messageQueue: [],
        },
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "sync_required",
      data: { reason: "lagged", skipped_messages: 3 },
    })

    await new Promise((resolve) => setTimeout(resolve, 0))

    const state = useChatStore.getState()
    expect(state.turns[0]?.turn_id).toBe("turn-resynced")
    assert.deepEqual(state.streamingTurn, {
      userMessages: ["继续执行"],
      status: "working",
      blocks: [{ type: "text", content: "已恢复中的输出" }],
    })
    expect(state.contextPressure).toBe(0.7)
    assert.deepEqual(
      state.sessions.map((session) => session.id),
      ["session-1", "session-2"]
    )

    globalThis.fetch = originalFetchImpl
  })

  test("turn_completed with cancelled outcome replaces streaming turn with preserved history", () => {
    useChatStore.setState({
      chatState: "active",
      streamingTurn: {
        userMessages: ["hello world"],
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
          latestTurn: null,
          streamingTurn: {
            userMessages: ["hello world"],
            status: "cancelled",
            blocks: [
              { type: "thinking", content: "先分析" },
              { type: "text", content: "部分回答" },
            ],
          },
          chatState: "idle",
          contextPressure: null,
          lastCompression: null,
          messageQueue: [],
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
        user_messages: ["hello world"],
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
    expect(state.chatState).toBe("idle")
    expect(state.streamingTurn).toBe(null)
    expect(state.turns.length).toBe(1)
    expect(state.turns[0]?.outcome).toBe("cancelled")
    expect(state.turns[0]?.assistant_message).toBe("部分回答")
    assert.equal(
      state._sessionSnapshots["session-1"]?.latestTurn?.turn_id,
      "turn-cancelled-1"
    )
    expect(state._sessionSnapshots["session-1"]?.streamingTurn).toBe(null)
  })

  test("switchSession hydrates latest turn first, idles in the second turn, then pages older history", async () => {
    const originalFetchImpl = globalThis.fetch
    let runIdleHydration: (() => void) | null = null
    let historyRequestCount = 0

    __setIdleSchedulerForTests({
      schedule: (callback) => {
        runIdleHydration = callback
        return 1
      },
      cancel: () => {
        runIdleHydration = null
      },
    })

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
      if (url.includes("/api/session/current-turn")) {
        return new Response("null", {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      }
      if (url.includes("/api/session/history")) {
        const searchParams = new URL(url, "http://localhost").searchParams
        historyRequestCount += 1
        if (historyRequestCount === 1) {
          expect(searchParams.get("session_id")).toBe("session-2")
          expect(searchParams.get("limit")).toBe("1")
          expect(searchParams.get("before_turn_id")).toBe(null)
          return new Response(
            JSON.stringify({
              turns: [
                {
                  turn_id: "turn-2-latest",
                  started_at_ms: 11,
                  finished_at_ms: 12,
                  source_entry_ids: [3],
                  user_messages: ["target latest question"],
                  blocks: [
                    { kind: "assistant", content: "target latest answer" },
                  ],
                  assistant_message: "target latest answer",
                  thinking: null,
                  tool_invocations: [],
                  usage: null,
                  failure_message: null,
                  outcome: "succeeded",
                },
              ],
              has_more: true,
              next_before_turn_id: "turn-2-latest",
            }),
            {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }
          )
        }
        if (historyRequestCount === 2) {
          expect(searchParams.get("session_id")).toBe("session-2")
          expect(searchParams.get("limit")).toBe("5")
          expect(searchParams.get("before_turn_id")).toBe("turn-2-latest")
          return new Response(
            JSON.stringify({
              turns: [
                {
                  turn_id: "turn-2-second",
                  started_at_ms: 9,
                  finished_at_ms: 10,
                  source_entry_ids: [4],
                  user_messages: ["target second question"],
                  blocks: [
                    { kind: "assistant", content: "target second answer" },
                  ],
                  assistant_message: "target second answer",
                  thinking: null,
                  tool_invocations: [],
                  usage: null,
                  failure_message: null,
                  outcome: "succeeded",
                },
              ],
              has_more: true,
              next_before_turn_id: "turn-2-second",
            }),
            {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }
          )
        }
        expect(historyRequestCount).toBe(3)
        expect(searchParams.get("session_id")).toBe("session-2")
        expect(searchParams.get("limit")).toBe("5")
        expect(searchParams.get("before_turn_id")).toBe("turn-2-second")
        return new Response(
          JSON.stringify({
            turns: [
              {
                turn_id: "turn-2-oldest",
                started_at_ms: 5,
                finished_at_ms: 6,
                source_entry_ids: [5],
                user_messages: ["target oldest question"],
                blocks: [
                  { kind: "assistant", content: "target oldest answer" },
                ],
                assistant_message: "target oldest answer",
                thinking: null,
                tool_invocations: [],
                usage: null,
                failure_message: null,
                outcome: "succeeded",
              },
              {
                turn_id: "turn-2-older",
                started_at_ms: 7,
                finished_at_ms: 8,
                source_entry_ids: [6],
                user_messages: ["target older question"],
                blocks: [{ kind: "assistant", content: "target older answer" }],
                assistant_message: "target older answer",
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
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      sessionHydrating: false,
      turns: [
        {
          turn_id: "turn-1-old",
          started_at_ms: 1,
          finished_at_ms: 2,
          source_entry_ids: [1],
          user_messages: ["older question"],
          blocks: [{ kind: "assistant", content: "older answer" }],
          assistant_message: "older answer",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        },
        {
          turn_id: "turn-1-latest",
          started_at_ms: 3,
          finished_at_ms: 4,
          source_entry_ids: [2],
          user_messages: ["latest question"],
          blocks: [{ kind: "assistant", content: "latest answer" }],
          assistant_message: "latest answer",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        },
      ],
    })

    const switchPromise = useChatStore.getState().switchSession("session-2")

    const duringHydration = useChatStore.getState()
    assert.equal(
      duringHydration._sessionSnapshots["session-1"]?.latestTurn?.turn_id,
      "turn-1-latest"
    )

    await switchPromise

    const firstHydrated = useChatStore.getState()
    expect(firstHydrated.turns.length).toBe(1)
    expect(firstHydrated.turns[0]?.turn_id).toBe("turn-2-latest")
    expect(firstHydrated.historyNextBeforeTurnId).toBe("turn-2-latest")

    requireResolver<() => void>(
      runIdleHydration as (() => void) | null,
      "expected idle hydration callback"
    )()

    await new Promise((resolve) => setTimeout(resolve, 0))

    const secondHydrated = useChatStore.getState()
    assert.deepEqual(
      secondHydrated.turns.map((turn) => turn.turn_id),
      ["turn-2-second", "turn-2-latest"]
    )
    expect(secondHydrated.historyNextBeforeTurnId).toBe("turn-2-second")

    await useChatStore.getState().loadOlderTurns()

    const pagedHistory = useChatStore.getState()
    assert.deepEqual(
      pagedHistory.turns.map((turn) => turn.turn_id),
      ["turn-2-oldest", "turn-2-older", "turn-2-second", "turn-2-latest"]
    )
    expect(pagedHistory.historyHasMore).toBe(false)

    globalThis.fetch = originalFetchImpl
  })

  test("switchSession keeps cached snapshot visible while hydrating next session", async () => {
    const originalFetchImpl = globalThis.fetch
    let resolveHistory: ResponseResolver | null = null
    let resolveCurrentTurn: ResponseResolver | null = null

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
          user_messages: ["old session question"],
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
          latestTurn: {
            turn_id: "turn-1",
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: [1],
            user_messages: ["old session question"],
            blocks: [{ kind: "assistant", content: "old session answer" }],
            assistant_message: "old session answer",
            thinking: null,
            tool_invocations: [],
            usage: null,
            failure_message: null,
            outcome: "succeeded",
          },
          streamingTurn: null,
          chatState: "idle",
          contextPressure: null,
          lastCompression: null,
          messageQueue: [],
        },
        "session-2": {
          latestTurn: {
            turn_id: "turn-2-cached",
            started_at_ms: 10,
            finished_at_ms: 20,
            source_entry_ids: [2],
            user_messages: ["cached question"],
            blocks: [{ kind: "assistant", content: "cached answer" }],
            assistant_message: "cached answer",
            thinking: null,
            tool_invocations: [],
            usage: null,
            failure_message: null,
            outcome: "succeeded",
          },
          streamingTurn: null,
          chatState: "idle",
          contextPressure: 0.1,
          lastCompression: null,
          messageQueue: [],
        },
      },
    })

    const switchPromise = useChatStore.getState().switchSession("session-2")

    const duringHydration = useChatStore.getState()
    expect(duringHydration.activeSessionId).toBe("session-2")
    expect(duringHydration.sessionHydrating).toBe(true)
    expect(duringHydration.turns[0]?.turn_id).toBe("turn-2-cached")
    expect(duringHydration.turns.length).toBe(1)

    requireResolver<ResponseResolver>(
      resolveHistory as ResponseResolver | null,
      "expected history resolver"
    )(
      new Response(
        JSON.stringify({
          turns: [
            {
              turn_id: "turn-2-live",
              started_at_ms: 11,
              finished_at_ms: 21,
              source_entry_ids: [3],
              user_messages: ["live question"],
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
    requireResolver<ResponseResolver>(
      resolveCurrentTurn as ResponseResolver | null,
      "expected current turn resolver"
    )(
      new Response("null", {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    )

    await switchPromise

    const hydrated = useChatStore.getState()
    expect(hydrated.sessionHydrating).toBe(false)
    expect(hydrated.turns[0]?.turn_id).toBe("turn-2-live")
    assert.equal(
      hydrated._sessionSnapshots["session-1"]?.latestTurn?.turn_id,
      "turn-1"
    )

    globalThis.fetch = originalFetchImpl
  })

  test("switchSession cancels delayed second-turn hydration when leaving session", async () => {
    const originalFetchImpl = globalThis.fetch
    let runIdleHydration: (() => void) | null = null
    let historyRequestCount = 0

    __setIdleSchedulerForTests({
      schedule: (callback) => {
        runIdleHydration = callback
        return 1
      },
      cancel: () => {
        runIdleHydration = null
      },
    })

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
      if (url.includes("/api/session/current-turn")) {
        return new Response("null", {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      }
      if (url.includes("session_id=session-3")) {
        return new Response(
          JSON.stringify({
            turns: [],
            has_more: false,
            next_before_turn_id: null,
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      }
      if (url.includes("/api/session/history")) {
        historyRequestCount += 1
        if (historyRequestCount === 1) {
          return new Response(
            JSON.stringify({
              turns: [
                {
                  turn_id: "turn-2-latest",
                  started_at_ms: 11,
                  finished_at_ms: 12,
                  source_entry_ids: [3],
                  user_messages: ["target latest question"],
                  blocks: [
                    { kind: "assistant", content: "target latest answer" },
                  ],
                  assistant_message: "target latest answer",
                  thinking: null,
                  tool_invocations: [],
                  usage: null,
                  failure_message: null,
                  outcome: "succeeded",
                },
              ],
              has_more: true,
              next_before_turn_id: "turn-2-latest",
            }),
            {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }
          )
        }
        return new Response(
          JSON.stringify({
            turns: [
              {
                turn_id: "turn-2-second",
                started_at_ms: 9,
                finished_at_ms: 10,
                source_entry_ids: [4],
                user_messages: ["target second question"],
                blocks: [
                  { kind: "assistant", content: "target second answer" },
                ],
                assistant_message: "target second answer",
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
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    const switchPromise = useChatStore.getState().switchSession("session-2")

    await switchPromise
    await useChatStore.getState().switchSession("session-3")

    const state = useChatStore.getState()
    expect(state.activeSessionId).not.toBe("session-2")
    expect(runIdleHydration).toBe(null)
    expect(historyRequestCount).toBe(1)
    assert.equal(
      state.turns.some((turn) => turn.turn_id === "turn-2-second"),
      false
    )

    globalThis.fetch = originalFetchImpl
  })

  test("session_deleted for active session swaps visible messages to next session snapshot", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      sessions: [
        {
          id: "session-1",
          title: "当前会话",
          title_source: "manual",
          auto_rename_policy: "enabled",
          created_at: "2026-03-17T00:00:00Z",
          updated_at: "2026-03-17T00:00:00Z",
          last_active_at: "2026-03-17T00:00:00Z",
          model: "gpt-4.1-mini",
        },
        {
          id: "session-2",
          title: "下一个会话",
          title_source: "manual",
          auto_rename_policy: "enabled",
          created_at: "2026-03-17T00:01:00Z",
          updated_at: "2026-03-17T00:01:00Z",
          last_active_at: "2026-03-17T00:01:00Z",
          model: "gpt-4.1-mini",
        },
      ],
      turns: [
        {
          turn_id: "turn-session-1",
          started_at_ms: 1,
          finished_at_ms: 2,
          source_entry_ids: [1],
          user_messages: ["当前会话问题"],
          blocks: [{ kind: "assistant", content: "当前会话答案" }],
          assistant_message: "当前会话答案",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        },
      ],
      streamingTurn: null,
      _sessionSnapshots: {
        "session-2": {
          latestTurn: {
            turn_id: "turn-session-2",
            started_at_ms: 10,
            finished_at_ms: 20,
            source_entry_ids: [2],
            user_messages: ["下一个会话问题"],
            blocks: [{ kind: "assistant", content: "下一个会话答案" }],
            assistant_message: "下一个会话答案",
            thinking: null,
            tool_invocations: [],
            usage: null,
            failure_message: null,
            outcome: "succeeded",
          },
          streamingTurn: null,
          chatState: "idle",
          contextPressure: 0.2,
          lastCompression: null,
          messageQueue: [],
        },
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "session_deleted",
      data: { session_id: "session-1" },
    })

    const state = useChatStore.getState()
    expect(state.activeSessionId).toBe("session-2")
    expect(state.turns).toHaveLength(1)
    expect(state.turns[0]?.turn_id).toBe("turn-session-2")
    expect(state.chatState).toBe("idle")
  })

  test("session_updated incrementally patches existing session metadata", () => {
    useChatStore.setState({
      sessions: [
        {
          id: "session-1",
          title: "旧标题",
          title_source: "default",
          auto_rename_policy: "enabled",
          created_at: "2026-03-17T00:00:00Z",
          updated_at: "2026-03-17T00:00:00Z",
          last_active_at: "2026-03-17T00:00:00Z",
          model: "gpt-4.1-mini",
        },
      ],
    })

    useChatStore.getState().handleSseEvent({
      type: "session_updated",
      data: {
        session_id: "session-1",
        title: "新标题",
        title_source: "auto",
        auto_rename_policy: "enabled",
        updated_at: "2026-03-17T00:10:00Z",
        last_active_at: "2026-03-17T00:09:00Z",
        model: "gpt-5",
      },
    })

    expect(useChatStore.getState().sessions[0]).toEqual({
      id: "session-1",
      title: "新标题",
      title_source: "auto",
      auto_rename_policy: "enabled",
      created_at: "2026-03-17T00:00:00Z",
      updated_at: "2026-03-17T00:10:00Z",
      last_active_at: "2026-03-17T00:09:00Z",
      model: "gpt-5",
    })
    expect(
      useChatStore.getState().sessionTitleAnimations["session-1"]?.animating
    ).toBe(true)
  })

  test("session_updated with unchanged title only refreshes activity without animation", () => {
    useChatStore.setState({
      sessions: [
        {
          id: "session-1",
          title: "稳定标题",
          title_source: "auto",
          auto_rename_policy: "enabled",
          created_at: "2026-03-17T00:00:00Z",
          updated_at: "2026-03-17T00:00:00Z",
          last_active_at: "2026-03-17T00:00:00Z",
          model: "gpt-4.1-mini",
        },
      ],
      sessionTitleAnimations: {
        "session-1": {
          targetTitle: "稳定标题",
          renderedTitle: "稳定标题",
          animating: false,
        },
      },
    })

    useChatStore.getState().handleSseEvent({
      type: "session_updated",
      data: {
        session_id: "session-1",
        title: "稳定标题",
        title_source: "auto",
        auto_rename_policy: "enabled",
        updated_at: "2026-03-17T00:10:00Z",
        last_active_at: "2026-03-17T00:09:00Z",
        model: "gpt-5",
      },
    })

    expect(useChatStore.getState().sessionTitleAnimations["session-1"]).toEqual(
      {
        targetTitle: "稳定标题",
        renderedTitle: "稳定标题",
        animating: false,
      }
    )
  })

  test("loadOlderTurns deduplicates overlapping page boundaries", async () => {
    const originalFetchImpl = globalThis.fetch

    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url.includes("/api/session/history")) {
        return new Response(
          JSON.stringify({
            turns: [
              {
                turn_id: "turn-older",
                started_at_ms: 1,
                finished_at_ms: 2,
                source_entry_ids: [1],
                user_messages: ["older question"],
                blocks: [{ kind: "assistant", content: "older answer" }],
                assistant_message: "older answer",
                thinking: null,
                tool_invocations: [],
                usage: null,
                failure_message: null,
                outcome: "succeeded",
              },
              {
                turn_id: "turn-current",
                started_at_ms: 11,
                finished_at_ms: 12,
                source_entry_ids: [2],
                user_messages: ["current question"],
                blocks: [{ kind: "assistant", content: "current answer" }],
                assistant_message: "current answer",
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
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      activeSessionId: "session-1",
      historyNextBeforeTurnId: "turn-current",
      historyHasMore: true,
      turns: [
        {
          turn_id: "turn-current",
          started_at_ms: 11,
          finished_at_ms: 12,
          source_entry_ids: [2],
          user_messages: ["current question"],
          blocks: [{ kind: "assistant", content: "current answer" }],
          assistant_message: "current answer",
          thinking: null,
          tool_invocations: [],
          usage: null,
          failure_message: null,
          outcome: "succeeded",
        },
      ],
      _sessionSnapshots: {
        "session-1": {
          latestTurn: {
            turn_id: "turn-current",
            started_at_ms: 11,
            finished_at_ms: 12,
            source_entry_ids: [2],
            user_messages: ["current question"],
            blocks: [{ kind: "assistant", content: "current answer" }],
            assistant_message: "current answer",
            thinking: null,
            tool_invocations: [],
            usage: null,
            failure_message: null,
            outcome: "succeeded",
          },
          streamingTurn: null,
          chatState: "idle",
          contextPressure: null,
          lastCompression: null,
          messageQueue: [],
        },
      },
    })

    await useChatStore.getState().loadOlderTurns()

    const state = useChatStore.getState()
    assert.deepEqual(
      state.turns.map((turn) => turn.turn_id),
      ["turn-older", "turn-current"]
    )
    assert.equal(
      state._sessionSnapshots["session-1"]?.latestTurn?.turn_id,
      "turn-current"
    )

    globalThis.fetch = originalFetchImpl
  })

  test("switchSessionModel refreshes providers and updates active session projection", async () => {
    const originalFetchImpl = globalThis.fetch

    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString()
      if (url === "/api/session/settings") {
        return new Response(
          JSON.stringify({
            name: "openai",
            model: "gpt-5-mini",
            connected: true,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        )
      }
      if (url === "/api/providers/list") {
        return new Response(
          JSON.stringify([
            {
              name: "openai",
              kind: "openai-responses",
              base_url: "https://api.openai.com",
              models: [
                {
                  id: "gpt-5-mini",
                  display_name: "GPT-5 Mini",
                  limit: null,
                  default_temperature: null,
                  supports_reasoning: true,
                },
              ],
            },
          ]),
          { status: 200, headers: { "Content-Type": "application/json" } }
        )
      }
      throw new Error(`unexpected fetch: ${url}`)
    }) as FetchMock

    useChatStore.setState({
      ...initialState,
      sessions: [
        {
          id: "session-1",
          title: "Session 1",
          title_source: "manual",
          auto_rename_policy: "enabled",
          created_at: "2026-03-21T00:00:00Z",
          updated_at: "2026-03-21T00:00:00Z",
          last_active_at: "2026-03-21T00:00:00Z",
          model: "gpt-5",
        },
      ],
    })

    useProviderRegistryStore.setState({
      providerList: [
        {
          name: "openai",
          kind: "openai-responses",
          base_url: "https://api.openai.com",
          models: [
            {
              id: "gpt-5",
              display_name: "GPT-5",
              limit: null,
              default_temperature: null,
              supports_reasoning: true,
            },
            {
              id: "gpt-5-mini",
              display_name: "GPT-5 Mini",
              limit: null,
              default_temperature: null,
              supports_reasoning: true,
            },
          ],
        },
      ],
    })
    useSessionSettingsStore.setState({
      sessionSettings: {
        provider: "openai",
        model: "gpt-5",
        protocol: "openai-responses",
        reasoning_effort: "high",
      },
      hydrating: false,
      updating: false,
      error: null,
    })

    await switchActiveSessionModel("openai", "gpt-5-mini")

    expect(useChatStore.getState().sessions[0]?.model).toBe("gpt-5-mini")
    expect(
      useProviderRegistryStore.getState().providerList[0]?.models[0]?.id
    ).toBe("gpt-5-mini")

    globalThis.fetch = originalFetchImpl
  })
})
