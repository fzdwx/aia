// Mirrors Rust StreamEvent (agent-core) — discriminated union on `kind`
export type StreamEvent =
  | { kind: "thinking_delta"; text: string }
  | { kind: "text_delta"; text: string }
  | {
      kind: "tool_call_detected"
      invocation_id: string
      tool_name: string
      arguments: Record<string, unknown> | null
    }
  | {
      kind: "tool_call_started"
      invocation_id: string
      tool_name: string
      arguments: Record<string, unknown> | null
    }
  | {
      kind: "tool_output_delta"
      invocation_id: string
      stream: "stdout" | "stderr"
      text: string
    }
  | { kind: "log"; text: string }
  | {
      kind: "tool_call_completed"
      invocation_id: string
      tool_name: string
      content: string
      details?: Record<string, unknown>
      failed: boolean
    }
  | { kind: "done" }

// Mirrors Rust ToolInvocationOutcome — discriminated union on `status`
export type ToolInvocationOutcome =
  | { status: "succeeded"; result: ToolResult }
  | { status: "failed"; message: string }

export type ToolResult = {
  invocation_id: string
  tool_name: string
  content: string
  response_id?: string
  details?: Record<string, unknown>
}

export type ToolCall = {
  invocation_id: string
  tool_name: string
  arguments: Record<string, unknown>
  response_id?: string
}

export type ToolTraceContext = {
  trace_id: string
  span_id: string
  parent_span_id: string
  root_span_id: string
  operation_name: string
  parent_request_kind: string
  parent_step_index: number
}

export type ToolInvocationLifecycle = {
  call: ToolCall
  started_at_ms: number
  finished_at_ms: number
  trace_context?: ToolTraceContext | null
  outcome: ToolInvocationOutcome
}

export type TurnUsage = {
  input_tokens: number
  output_tokens: number
  total_tokens: number
  cached_tokens: number
}

// Mirrors Rust TurnBlock — discriminated union on `kind`
export type TurnBlock =
  | { kind: "thinking"; content: string }
  | { kind: "assistant"; content: string }
  | { kind: "tool_invocation"; invocation: ToolInvocationLifecycle }
  | { kind: "failure"; message: string }

export type TurnLifecycle = {
  turn_id: string
  started_at_ms: number
  finished_at_ms: number
  source_entry_ids: number[]
  user_message: string
  blocks: TurnBlock[]
  assistant_message: string | null
  thinking: string | null
  tool_invocations: ToolInvocationLifecycle[]
  usage: TurnUsage | null
  failure_message: string | null
}

export type ProviderInfo = {
  name: string
  model: string
  connected: boolean
}

// Session list item from GET /api/sessions
export type SessionListItem = {
  id: string
  title: string
  created_at: string
  updated_at: string
  model: string
}

// SSE event types from the global /api/events stream — all carry session_id
export type SseEvent =
  | { type: "stream"; data: StreamEvent & { session_id: string } }
  | { type: "status"; data: { session_id: string; status: TurnStatus } }
  | {
      type: "turn_completed"
      data: TurnLifecycle & { session_id: string }
    }
  | {
      type: "context_compressed"
      data: { session_id: string; summary: string }
    }
  | { type: "error"; data: { session_id: string; message: string } }
  | {
      type: "session_created"
      data: { session_id: string; title: string }
    }
  | { type: "session_deleted"; data: { session_id: string } }
  | { type: "turn_cancelled"; data: { session_id: string } }

// Mirrors Rust TurnStatus
export type TurnStatus =
  | "waiting"
  | "thinking"
  | "working"
  | "generating"
  | "cancelled"

export type CurrentToolOutput = {
  invocation_id: string
  tool_name: string
  arguments: Record<string, unknown>
  detected_at_ms: number
  started_at_ms: number | null
  finished_at_ms: number | null
  output: string
  completed: boolean
  result_content: string | null
  result_details: Record<string, unknown> | null
  failed: boolean | null
}

export type CurrentTurnBlock =
  | { kind: "thinking"; content: string }
  | { kind: "tool"; tool: CurrentToolOutput }
  | { kind: "text"; content: string }

export type CurrentTurnSnapshot = {
  started_at_ms: number
  user_message: string
  status: TurnStatus
  blocks: CurrentTurnBlock[]
}

// Streaming tool output accumulator
export type StreamingToolOutput = {
  invocationId: string
  toolName: string
  arguments: Record<string, unknown>
  detectedAtMs: number
  startedAtMs?: number
  finishedAtMs?: number
  output: string
  completed: boolean
  resultContent?: string
  resultDetails?: Record<string, unknown>
  failed?: boolean
}

// Ordered streaming block — mirrors the real event sequence
export type StreamingBlock =
  | { type: "thinking"; content: string }
  | { type: "tool"; tool: StreamingToolOutput }
  | { type: "text"; content: string }

// Streaming turn accumulator state
export type StreamingTurn = {
  userMessage: string
  status: TurnStatus
  thinkingText?: string
  assistantText?: string
  toolOutputs?: StreamingToolOutput[]
  blocks: StreamingBlock[]
}

export type ChatState = "idle" | "active"

export type ModelConfig = {
  id: string
  display_name: string | null
  limit: {
    context: number | null
    output: number | null
  } | null
  default_temperature: number | null
  supports_reasoning: boolean
  reasoning_effort: string | null
}

export type ProviderListItem = {
  name: string
  kind: string
  models: ModelConfig[]
  active_model: string | null
  base_url: string
  active: boolean
}

export type TraceStatus = "succeeded" | "failed"
export type TraceSpanKind = "CLIENT" | "INTERNAL"
export type TraceEvent = {
  name: string
  at_ms: number
  attributes: Record<string, unknown> | null
}

export type TraceListItem = {
  id: string
  trace_id: string
  span_id: string
  parent_span_id: string | null
  root_span_id: string
  operation_name: string
  span_kind: TraceSpanKind
  turn_id: string
  run_id: string
  request_kind: string
  step_index: number
  provider: string
  protocol: string
  model: string
  endpoint_path: string
  status: TraceStatus
  stop_reason: string | null
  status_code: number | null
  started_at_ms: number
  duration_ms: number | null
  total_tokens: number | null
  cached_tokens: number | null
  user_message: string | null
  error: string | null
}

export type TraceListPage = {
  items: TraceListItem[]
  total_loops: number
  page: number
  page_size: number
}

export type TraceRecord = {
  id: string
  trace_id: string
  span_id: string
  parent_span_id: string | null
  root_span_id: string
  operation_name: string
  span_kind: TraceSpanKind
  turn_id: string
  run_id: string
  request_kind: string
  step_index: number
  provider: string
  protocol: string
  model: string
  base_url: string
  endpoint_path: string
  streaming: boolean
  started_at_ms: number
  finished_at_ms: number | null
  duration_ms: number | null
  status_code: number | null
  status: TraceStatus
  stop_reason: string | null
  error: string | null
  request_summary: Record<string, unknown> | null
  provider_request: Record<string, unknown> | null
  response_summary: Record<string, unknown> | null
  response_body: string | null
  input_tokens: number | null
  output_tokens: number | null
  total_tokens: number | null
  cached_tokens: number | null
  otel_attributes: Record<string, unknown> | null
  events: TraceEvent[]
}

export type TraceSummary = {
  total_requests: number
  failed_requests: number
  avg_duration_ms: number | null
  p95_duration_ms: number | null
  total_input_tokens: number
  total_output_tokens: number
  total_tokens: number
  total_cached_tokens: number
}

export type AppView = "chat" | "settings" | "trace"
