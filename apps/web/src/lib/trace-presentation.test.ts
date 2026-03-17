import { describe, expect, test } from "vite-plus/test"

import {
  buildTraceLoopGroups,
  partitionTraceLoopGroups,
} from "@/lib/trace-presentation"
import type { TraceListItem } from "@/lib/types"

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

describe("trace presentation", () => {
  test("marks compression traces as compression activity", () => {
    const groups = buildTraceLoopGroups([compressionTrace()], [])

    expect(groups).toHaveLength(1)
    expect(groups[0]?.requestKind).toBe("compression")
  })

  test("separates compression groups from conversation groups", () => {
    const groups = buildTraceLoopGroups(
      [
        compressionTrace(),
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
        }),
      ],
      []
    )

    const partitioned = partitionTraceLoopGroups(groups)

    expect(partitioned.compression).toHaveLength(1)
    expect(partitioned.conversation).toHaveLength(1)
    expect(partitioned.compression[0]?.requestKind).toBe("compression")
    expect(partitioned.conversation[0]?.requestKind).toBe("completion")
  })
})
