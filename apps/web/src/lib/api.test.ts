import { afterEach, expect, test, vi } from "vite-plus/test"

import { connectEvents, sendWidgetClientEvent } from "./api"

type Listener = (event: MessageEvent) => void

class MockEventSource {
  static instance: MockEventSource | null = null

  private listeners = new Map<string, Set<Listener>>()

  constructor(_url: string) {
    MockEventSource.instance = this
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject) {
    const callback = listener as Listener
    const listeners = this.listeners.get(type) ?? new Set<Listener>()
    listeners.add(callback)
    this.listeners.set(type, listeners)
  }

  close() {}

  emit(type: string, data: unknown) {
    const listeners = this.listeners.get(type)
    if (!listeners) return
    const event = { data: JSON.stringify(data) } as MessageEvent
    for (const listener of listeners) listener(event)
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  MockEventSource.instance = null
})

test("sendWidgetClientEvent posts structured widget event payload", async () => {
  const fetchMock = vi.fn(async () => ({
    ok: true,
    status: 200,
  }))
  vi.stubGlobal("fetch", fetchMock)

  await sendWidgetClientEvent({
    session_id: "session-1",
    turn_id: "turn-1",
    invocation_id: "widget-1",
    event: {
      type: "scripts_ready",
    },
  })

  expect(fetchMock).toHaveBeenCalledWith("/api/session/widget-event", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      session_id: "session-1",
      turn_id: "turn-1",
      invocation_id: "widget-1",
      event: {
        type: "scripts_ready",
      },
    }),
  })
})

test("connectEvents maps default SSE message error envelopes to chat error events", () => {
  vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource)
  const onEvent = vi.fn()

  const dispose = connectEvents(onEvent)
  MockEventSource.instance?.emit("message", {
    error: {
      type: "rate_limit_error",
      message: "Concurrency limit exceeded for user, please retry later",
    },
  })

  expect(onEvent).toHaveBeenCalledWith({
    type: "error",
    data: {
      session_id: null,
      turn_id: null,
      message: "Concurrency limit exceeded for user, please retry later",
      error_type: "rate_limit_error",
    },
  })

  dispose()
})

test("connectEvents batches adjacent text stream deltas before dispatch", async () => {
  vi.useFakeTimers()
  vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource)
  const onEvent = vi.fn()

  const dispose = connectEvents(onEvent)

  MockEventSource.instance?.emit("stream", {
    session_id: "session-1",
    turn_id: "turn-1",
    kind: "text_delta",
    text: "hello",
  })
  MockEventSource.instance?.emit("stream", {
    session_id: "session-1",
    turn_id: "turn-1",
    kind: "text_delta",
    text: " world",
  })

  expect(onEvent).not.toHaveBeenCalled()

  await vi.advanceTimersByTimeAsync(16)

  expect(onEvent).toHaveBeenCalledTimes(1)
  expect(onEvent).toHaveBeenCalledWith({
    type: "stream",
    data: {
      session_id: "session-1",
      turn_id: "turn-1",
      kind: "text_delta",
      text: "hello world",
    },
  })

  dispose()
  vi.useRealTimers()
})

test("connectEvents flushes pending text delta before non-text stream event", () => {
  vi.useFakeTimers()
  vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource)
  const onEvent = vi.fn()

  const dispose = connectEvents(onEvent)

  MockEventSource.instance?.emit("stream", {
    session_id: "session-1",
    turn_id: "turn-1",
    kind: "thinking_delta",
    text: "step 1",
  })
  MockEventSource.instance?.emit("stream", {
    session_id: "session-1",
    turn_id: "turn-1",
    kind: "tool_call_started",
    invocation_id: "Shell:1",
    tool_name: "Shell",
    arguments: { command: "echo ok" },
    started_at_ms: 123,
  })

  expect(onEvent).toHaveBeenCalledTimes(2)
  expect(onEvent).toHaveBeenNthCalledWith(1, {
    type: "stream",
    data: {
      session_id: "session-1",
      turn_id: "turn-1",
      kind: "thinking_delta",
      text: "step 1",
    },
  })
  expect(onEvent).toHaveBeenNthCalledWith(2, {
    type: "stream",
    data: {
      session_id: "session-1",
      turn_id: "turn-1",
      kind: "tool_call_started",
      invocation_id: "Shell:1",
      tool_name: "Shell",
      arguments: { command: "echo ok" },
      started_at_ms: 123,
    },
  })

  dispose()
  vi.useRealTimers()
})
