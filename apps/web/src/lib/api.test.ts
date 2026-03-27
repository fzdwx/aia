import { afterEach, expect, test, vi } from "vite-plus/test"

import { connectEvents } from "./api"

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
