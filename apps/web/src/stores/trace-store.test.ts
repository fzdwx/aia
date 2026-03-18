import { afterEach, beforeEach, describe, expect, test } from "vite-plus/test"

import { useTraceStore } from "./trace-store"

type FetchMock = typeof fetch

const initialState = {
  traces: [],
  traceView: "conversation" as const,
  tracePage: 1,
  tracePageSize: 12,
  totalTraceItems: 0,
  selectedTraceId: null,
  selectedTrace: null,
  selectedLoop: null,
  traceSummary: null,
  traceLoading: false,
  traceError: null,
}

describe("trace store", () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    useTraceStore.setState(initialState)
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  test("coalesces duplicate overview refreshes for the same view", async () => {
    let fetchCount = 0

    globalThis.fetch = (async (input) => {
      fetchCount += 1
      const url = String(input)
      expect(url).toContain("/api/traces/overview")
      return new Promise<Response>((resolve) => {
        setTimeout(() => {
          resolve(
            new Response(
              JSON.stringify({
                summary: {
                  total_requests: 1,
                  failed_requests: 0,
                  avg_duration_ms: 25,
                  p95_duration_ms: 25,
                  total_input_tokens: 10,
                  total_output_tokens: 5,
                  total_tokens: 15,
                  total_cached_tokens: 0,
                },
                page: {
                  items: [
                    {
                      id: "loop-1",
                      trace_id: "loop-1",
                      turn_id: "turn-1",
                      run_id: "run-1",
                      root_span_id: "root-1",
                      request_kind: "completion",
                      model: "gpt-5.4",
                      protocol: "openai-responses",
                      endpoint_path: "/responses",
                      latest_started_at_ms: 100,
                      started_at_ms: 100,
                      finished_at_ms: 125,
                      duration_ms: 25,
                      total_tokens: 15,
                      total_cached_tokens: 0,
                      llm_span_count: 1,
                      tool_span_count: 0,
                      failed_tool_count: 0,
                      final_status: "completed",
                      user_message: "hello",
                      latest_error: null,
                      final_span_id: "trace-1",
                      traces: [
                        {
                          id: "trace-1",
                          trace_id: "loop-1",
                          span_id: "trace-1",
                          parent_span_id: null,
                          root_span_id: "root-1",
                          operation_name: "chat",
                          span_kind: "CLIENT",
                          turn_id: "turn-1",
                          run_id: "run-1",
                          request_kind: "completion",
                          step_index: 0,
                          provider: "openai",
                          protocol: "openai-responses",
                          model: "gpt-5.4",
                          endpoint_path: "/responses",
                          status: "succeeded",
                          stop_reason: "stop",
                          status_code: 200,
                          started_at_ms: 100,
                          duration_ms: 25,
                          total_tokens: 15,
                          cached_tokens: 0,
                          user_message: "hello",
                          error: null,
                        },
                      ],
                    },
                  ],
                  total_items: 1,
                  page: 1,
                  page_size: 12,
                },
              }),
              { status: 200 }
            )
          )
        }, 0)
      })
    }) as FetchMock

    await Promise.all([
      useTraceStore.getState().refreshTraces(),
      useTraceStore.getState().refreshTraces(),
    ])

    expect(fetchCount).toBe(1)
    expect(useTraceStore.getState().traces).toHaveLength(1)
    expect(useTraceStore.getState().traceSummary?.total_requests).toBe(1)
  })
})
