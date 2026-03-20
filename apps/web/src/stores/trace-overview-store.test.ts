import { afterEach, beforeEach, describe, expect, test } from "vite-plus/test"

import { useTraceOverviewStore } from "./trace-overview-store"

type FetchMock = typeof fetch

const dashboardPayload = {
  range: "month",
  current: {
    total_cost_usd: 12.5,
    total_requests: 10,
    failed_requests: 2,
    partial_requests: 1,
    total_sessions: 4,
    total_input_tokens: 120,
    total_output_tokens: 80,
    total_tokens: 200,
    total_cached_tokens: 40,
    total_lines_added: 12,
    total_lines_removed: 5,
    total_lines_changed: 17,
  },
  previous: {
    total_cost_usd: 8,
    total_requests: 8,
    failed_requests: 1,
    partial_requests: 1,
    total_sessions: 3,
    total_input_tokens: 100,
    total_output_tokens: 70,
    total_tokens: 170,
    total_cached_tokens: 30,
    total_lines_added: 6,
    total_lines_removed: 2,
    total_lines_changed: 8,
  },
  trend: [
    {
      bucket_start_ms: 1,
      total_requests: 3,
      failed_requests: 1,
      partial_requests: 0,
      total_input_tokens: 40,
      total_output_tokens: 20,
      total_cached_tokens: 10,
      total_tokens: 60,
    },
  ],
  activity: [],
  overall_summary: {
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
    total_input_tokens: 120,
    total_output_tokens: 80,
    total_tokens: 200,
    total_cached_tokens: 40,
  },
  conversation_summary: {
    total_requests: 8,
    failed_requests: 1,
    partial_requests: 1,
    avg_duration_ms: 20,
    p95_duration_ms: 32,
    total_llm_spans: 14,
    total_tool_spans: 8,
    requests_with_tools: 4,
    failed_tool_calls: 1,
    unique_models: 2,
    latest_request_started_at_ms: 1000,
    total_input_tokens: 100,
    total_output_tokens: 70,
    total_tokens: 170,
    total_cached_tokens: 30,
  },
  compression_summary: {
    total_requests: 2,
    failed_requests: 0,
    partial_requests: 1,
    avg_duration_ms: 12,
    p95_duration_ms: 15,
    total_llm_spans: 2,
    total_tool_spans: 0,
    requests_with_tools: 0,
    failed_tool_calls: 0,
    unique_models: 1,
    latest_request_started_at_ms: 900,
    total_input_tokens: 20,
    total_output_tokens: 10,
    total_tokens: 30,
    total_cached_tokens: 10,
  },
}

describe("trace overview store", () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    useTraceOverviewStore.setState({
      dashboard: null,
      range: "month",
      loading: false,
      initialized: false,
      error: null,
    })
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  test("loads dashboard payload for the selected range", async () => {
    const requests: string[] = []

    globalThis.fetch = (async (input) => {
      const url = String(input)
      requests.push(url)

      return new Response(JSON.stringify(dashboardPayload), { status: 200 })
    }) as FetchMock

    await useTraceOverviewStore.getState().refresh("week")

    expect(requests).toHaveLength(1)
    expect(requests[0]).toBe("/api/traces/dashboard?range=week")
    expect(
      useTraceOverviewStore.getState().dashboard?.current.total_requests
    ).toBe(10)
    expect(
      useTraceOverviewStore.getState().dashboard?.current.total_lines_changed
    ).toBe(17)
    expect(useTraceOverviewStore.getState().range).toBe("week")
  })
})
