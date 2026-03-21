import { afterEach, beforeEach, describe, expect, test } from "vite-plus/test"

import { useSessionSettingsStore } from "./session-settings-store"
import { useChatStore } from "./chat-store"
import type { ProviderListItem } from "@/lib/types"

type FetchMock = typeof fetch

const originalFetch = globalThis.fetch

const providerList: ProviderListItem[] = [
  {
    name: "openai",
    kind: "openai-responses",
    base_url: "https://api.openai.com",
    active: true,
    models: [
      {
        id: "gpt-5",
        display_name: "GPT-5",
        limit: null,
        default_temperature: null,
        supports_reasoning: true,
      },
      {
        id: "gpt-4.1-mini",
        display_name: "GPT-4.1 Mini",
        limit: null,
        default_temperature: null,
        supports_reasoning: false,
      },
    ],
  },
]

describe("session settings store", () => {
  beforeEach(() => {
    useSessionSettingsStore.setState({
      activeSessionId: null,
      sessionSettings: null,
      hydrating: false,
      updating: false,
      error: null,
    })
    useChatStore.setState({
      sessions: [
        {
          id: "session-1",
          title: "Session 1",
          created_at: "2026-03-21T00:00:00Z",
          updated_at: "2026-03-21T00:00:00Z",
          model: "gpt-5",
        },
      ],
    })
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  test("supportsReasoning reflects active session model capability", () => {
    useSessionSettingsStore.setState({
      activeSessionId: "session-1",
      sessionSettings: {
        provider: "openai",
        model: "gpt-5",
        protocol: "openai-responses",
        reasoning_effort: "high",
      },
      hydrating: false,
    })

    expect(
      useSessionSettingsStore.getState().supportsReasoning(providerList)
    ).toBe(true)

    useSessionSettingsStore.setState({
      sessionSettings: {
        provider: "openai",
        model: "gpt-4.1-mini",
        protocol: "openai-responses",
        reasoning_effort: null,
      },
    })

    expect(
      useSessionSettingsStore.getState().supportsReasoning(providerList)
    ).toBe(false)
  })

  test("switchModel updates session-scoped settings and clears reasoning for unsupported models", async () => {
    const calls: Array<{ url: string; body?: string }> = []
    globalThis.fetch = (async (
      input: RequestInfo | URL,
      init?: RequestInit
    ) => {
      const url = typeof input === "string" ? input : input.toString()
      calls.push({ url, body: init?.body as string | undefined })
      return new Response(
        JSON.stringify({
          name: "openai",
          model: "gpt-4.1-mini",
          connected: true,
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      )
    }) as FetchMock

    useSessionSettingsStore.setState({
      activeSessionId: "session-1",
      sessionSettings: {
        provider: "openai",
        model: "gpt-5",
        protocol: "openai-responses",
        reasoning_effort: "high",
      },
      hydrating: false,
    })

    await useSessionSettingsStore
      .getState()
      .switchModel(providerList, "openai", "gpt-4.1-mini", "xhigh")

    expect(calls[0]?.url).toBe("/api/session/settings")
    expect(JSON.parse(calls[0]?.body ?? "{}")).toEqual({
      session_id: "session-1",
      provider: "openai",
      model: "gpt-4.1-mini",
      reasoning_effort: null,
    })
    expect(useSessionSettingsStore.getState().sessionSettings).toEqual({
      provider: "openai",
      model: "gpt-4.1-mini",
      protocol: "openai-responses",
      reasoning_effort: null,
    })
    expect(useChatStore.getState().sessions[0]?.model).toBe("gpt-4.1-mini")
  })

  test("hydrateForSession surfaces loading failure", async () => {
    globalThis.fetch = (async () =>
      new Response(null, { status: 500 })) as FetchMock

    await useSessionSettingsStore.getState().hydrateForSession("session-1")

    expect(useSessionSettingsStore.getState().hydrating).toBe(false)
    expect(useSessionSettingsStore.getState().error).toContain(
      "GET /api/session/settings failed"
    )
  })

  test("switchModel resets updating flag after request failure", async () => {
    globalThis.fetch = (async () =>
      new Response(null, { status: 500 })) as FetchMock

    useSessionSettingsStore.setState({
      activeSessionId: "session-1",
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

    await expect(
      useSessionSettingsStore
        .getState()
        .switchModel(providerList, "openai", "gpt-5", "xhigh")
    ).rejects.toThrow()

    expect(useSessionSettingsStore.getState().updating).toBe(false)
    expect(useSessionSettingsStore.getState().error).toContain(
      "PUT /api/session/settings failed"
    )
  })
})
