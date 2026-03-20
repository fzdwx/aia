import { afterEach, beforeEach, describe, expect, test } from "vite-plus/test"

import { useTraceOverviewStore } from "./trace-overview-store"

type FetchMock = typeof fetch

const summaryPayload = {
  total_requests: 10,
  failed_requests: 1,
  partial_requests: 2,
  avg_duration_ms: 24,
  p95_duration_ms: 40,
  total_llm_spans: 16,
  total_tool_spans: 8,
  requests_with_tools: 4,
  failed_tool_calls: 1,
  unique_models: 2,
  latest_request_started_at_ms: 1000,
  total_input_tokens: 100,
  total_output_tokens: 80,
  total_tokens: 180,
  total_cached_tokens: 20,
}

describe("trace overview store", () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    useTraceOverviewStore.setState({
      overallSummary: null,
      conversationSummary: null,
      compressionSummary: null,
      loading: false,
      initialized: false,
      error: null,
    })
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  test("loads cumulative summaries for overall conversation and compression", async () => {
    const requests: string[] = []

    globalThis.fetch = (async (input) => {
      const url = String(input)
      requests.push(url)

      return new Response(JSON.stringify(summaryPayload), { status: 200 })
    }) as FetchMock

    await useTraceOverviewStore.getState().refresh()

    expect(requests).toHaveLength(3)
    expect(requests[0]).toBe("/api/traces/summary")
    expect(requests[1]).toContain("request_kind=completion")
    expect(requests[2]).toContain("request_kind=compression")
    expect(
      useTraceOverviewStore.getState().overallSummary?.total_requests
    ).toBe(10)
    expect(
      useTraceOverviewStore.getState().conversationSummary?.total_tool_spans
    ).toBe(8)
    expect(
      useTraceOverviewStore.getState().compressionSummary
        ?.latest_request_started_at_ms
    ).toBe(1000)
  })
})
