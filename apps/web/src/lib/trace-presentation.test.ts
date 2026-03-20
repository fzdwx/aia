import { describe, expect, test } from "vite-plus/test"

import {
  buildTraceLoopGroups,
  formatTraceDuration,
  formatTraceLoopHeadline,
  resolveActiveTraceLoopKey,
  selectVisibleTraceLoopGroups,
} from "@/lib/trace-presentation"
import type { TraceListItem, TraceLoopItem } from "@/lib/types"

function compressionTrace(overrides?: Partial<TraceListItem>): TraceListItem {
  return {
    id: "trace-compression-1",
    trace_id: "trace-compression-group",
    span_id: "trace-compression-1",
    parent_span_id: "trace-compression-root",
    root_span_id: "trace-compression-root",
    operation_name: "summarize",
    span_kind: "CLIENT",
    turn_id: "compression-1",
    run_id: "compression-1",
    request_kind: "compression",
    step_index: 0,
    provider: "openai",
    protocol: "openai-responses",
    model: "gpt-5.4",
    endpoint_path: "/responses",
    status: "succeeded",
    stop_reason: "stop",
    status_code: 200,
    started_at_ms: 100,
    duration_ms: 40,
    total_tokens: 12,
    cached_tokens: 0,
    user_message: null,
    error: null,
    ...overrides,
  }
}

function loopFromTrace(
  trace: TraceListItem,
  overrides?: Partial<TraceLoopItem>
): TraceLoopItem {
  return {
    id: trace.trace_id,
    trace_id: trace.trace_id,
    request_kind: trace.request_kind,
    turn_id: trace.turn_id,
    run_id: trace.run_id,
    root_span_id: trace.root_span_id,
    model: trace.model,
    protocol: trace.protocol,
    endpoint_path: trace.endpoint_path,
    latest_started_at_ms: trace.started_at_ms,
    started_at_ms: trace.started_at_ms,
    finished_at_ms:
      trace.duration_ms != null
        ? trace.started_at_ms + trace.duration_ms
        : null,
    duration_ms: trace.duration_ms,
    total_tokens: trace.total_tokens ?? 0,
    total_cached_tokens: trace.cached_tokens ?? 0,
    llm_span_count: 1,
    tool_span_count: 0,
    failed_tool_count: 0,
    final_status: "completed",
    user_message: trace.user_message,
    latest_error: trace.error,
    final_span_id: trace.id,
    traces: [trace],
    ...overrides,
  }
}

describe("trace presentation", () => {
  test("marks compression traces as compression activity", () => {
    const groups = buildTraceLoopGroups([loopFromTrace(compressionTrace())], [])

    expect(groups).toHaveLength(1)
    expect(groups[0]?.requestKind).toBe("compression")
  })

  test("selects compression groups separately from conversation groups", () => {
    const groups = buildTraceLoopGroups(
      [
        loopFromTrace(compressionTrace()),
        loopFromTrace(
          compressionTrace({
            id: "trace-chat-1",
            trace_id: "trace-chat-group",
            span_id: "trace-chat-1",
            root_span_id: "trace-chat-root",
            parent_span_id: "trace-chat-root",
            turn_id: "turn-chat-1",
            run_id: "turn-chat-1",
            operation_name: "chat",
            request_kind: "completion",
            user_message: "hello",
          })
        ),
      ],
      []
    )

    const compression = selectVisibleTraceLoopGroups(groups, "compression")
    const conversation = selectVisibleTraceLoopGroups(groups, "conversation")

    expect(compression).toHaveLength(1)
    expect(conversation).toHaveLength(1)
    expect(compression[0]?.requestKind).toBe("compression")
    expect(conversation[0]?.requestKind).toBe("completion")
  })

  test("falls back to the first visible group when active loop is missing", () => {
    const groups = buildTraceLoopGroups(
      [
        loopFromTrace(compressionTrace()),
        loopFromTrace(
          compressionTrace({
            id: "trace-chat-1",
            trace_id: "trace-chat-group",
            span_id: "trace-chat-1",
            root_span_id: "trace-chat-root",
            parent_span_id: "trace-chat-root",
            turn_id: "turn-chat-1",
            run_id: "turn-chat-1",
            operation_name: "chat",
            request_kind: "completion",
            user_message: "hello",
          })
        ),
      ],
      []
    )

    const conversation = selectVisibleTraceLoopGroups(groups, "conversation")
    expect(resolveActiveTraceLoopKey(conversation, "missing-loop")).toBe(
      "trace-chat-group"
    )
  })

  test("formats loop headlines with configurable compression labels", () => {
    const [compressionGroup] = buildTraceLoopGroups(
      [loopFromTrace(compressionTrace())],
      []
    )

    expect(
      formatTraceLoopHeadline(compressionGroup!, {
        compressionLabel: "Context compression log",
      })
    ).toBe("Context compression log")
  })

  test("formats long trace durations consistently", () => {
    expect(formatTraceDuration(90_000)).toBe("1m 30s")
  })
})
