// Mirrors Rust StreamEvent (agent-core) — discriminated union on `kind`
export type StreamEvent =
  | { kind: "thinking_delta"; text: string }
  | { kind: "text_delta"; text: string }
  | {
      kind: "tool_output_delta"
      invocation_id: string
      stream: "stdout" | "stderr"
      text: string
    }
  | { kind: "log"; text: string }
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
}

export type ToolCall = {
  invocation_id: string
  tool_name: string
  arguments: Record<string, unknown>
  response_id?: string
}

export type ToolInvocationLifecycle = {
  call: ToolCall
  outcome: ToolInvocationOutcome
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
  failure_message: string | null
}

export type ProviderInfo = {
  name: string
  model: string
  connected: boolean
}

// SSE event types from the global /api/events stream
export type SseEvent =
  | { type: "stream"; data: StreamEvent }
  | { type: "status"; data: { status: TurnStatus } }
  | { type: "turn_completed"; data: TurnLifecycle }
  | { type: "error"; data: { message: string } }

// Mirrors Rust TurnStatus
export type TurnStatus = "waiting" | "thinking" | "working" | "generating"

// Streaming turn accumulator state
export type StreamingTurn = {
  userMessage: string
  thinkingText: string
  assistantText: string
  status: TurnStatus
}

export type ChatState = "idle" | "active"
