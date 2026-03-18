import type { TraceListItem, TraceLoopItem, TurnLifecycle } from "@/lib/types"

export type LoopStatus = "completed" | "failed" | "partial"

export type AgentRootNode = {
  kind: "agent_root"
  id: string
  name: string
  operationName: "invoke_agent"
  spanKind: "INTERNAL"
  startedAtMs: number
  finishedAtMs: number | null
  durationMs: number
  status: LoopStatus
  userMessage: string | null
}

export type LlmSpanNode = {
  kind: "llm_span"
  id: string
  trace: TraceListItem
  name: string
  operationName: string
  spanKind: TraceListItem["span_kind"]
  startedAtMs: number
  finishedAtMs: number | null
  durationMs: number
  status: "ok" | "error"
  toolCount: number
}

export type ToolSpanNode = {
  kind: "tool_span"
  id: string
  trace: TraceListItem
  name: string
  operationName: "execute_tool"
  spanKind: "INTERNAL"
  startedAtMs: number
  finishedAtMs: number | null
  durationMs: number
  status: "ok" | "error"
}

export type LoopTimelineNode = AgentRootNode | LlmSpanNode | ToolSpanNode

export type TraceLoopGroup = {
  key: string
  requestKind: string
  turnId: string
  runId: string
  userMessage: string | null
  assistantMessage: string | null
  model: string
  protocol: string
  endpointPath: string
  latestStartedAtMs: number
  startedAtMs: number
  finishedAtMs: number | null
  totalDurationMs: number
  totalTokens: number
  stepCount: number
  toolCount: number
  failedToolCount: number
  finalStatus: LoopStatus
  traces: TraceListItem[]
  turn: TurnLifecycle | null
  pathSummary: string
  latestError: string | null
  timeline: LoopTimelineNode[]
  finalSpanId: string | null
}

export function isCompressionRequestKind(requestKind: string) {
  return requestKind === "compression"
}

export function partitionTraceLoopGroups(groups: TraceLoopGroup[]) {
  const conversation: TraceLoopGroup[] = []
  const compression: TraceLoopGroup[] = []

  for (const group of groups) {
    if (isCompressionRequestKind(group.requestKind)) {
      compression.push(group)
    } else {
      conversation.push(group)
    }
  }

  return { conversation, compression }
}

function isToolTrace(trace: TraceListItem) {
  return trace.request_kind === "tool"
}

function summarizeStopReason(trace: TraceListItem): string | null {
  if (trace.status === "failed") {
    return isToolTrace(trace) ? "tool.failed" : "failed"
  }

  if (isToolTrace(trace)) {
    return "tool.completed"
  }

  if (trace.stop_reason && trace.stop_reason !== "completed") {
    return trace.stop_reason
  }

  return null
}

function llmSpanName(trace: TraceListItem): string {
  return `${trace.operation_name} ${trace.model}`
}

function toolSpanName(trace: TraceListItem): string {
  return `execute_tool ${trace.model}`
}

function spanStatus(trace: TraceListItem): "ok" | "error" {
  return trace.status === "failed" ? "error" : "ok"
}

function loopStatus(llmTraces: TraceListItem[], toolTraces: TraceListItem[]) {
  const finalLlmTrace = llmTraces[llmTraces.length - 1] ?? null
  const hasTraceFailure = llmTraces.some((trace) => trace.status === "failed")
  const hasToolFailure = toolTraces.some((trace) => trace.status === "failed")

  if (finalLlmTrace?.status === "failed") return "failed" as const
  if (hasTraceFailure || hasToolFailure) return "partial" as const
  return "completed" as const
}

function findFinishedAtMs(trace: TraceListItem): number | null {
  if (trace.duration_ms == null) return null
  return trace.started_at_ms + trace.duration_ms
}

function sortByStartedAt<T extends TraceListItem>(traces: T[]) {
  return [...traces].sort((left, right) => {
    if (left.started_at_ms !== right.started_at_ms) {
      return left.started_at_ms - right.started_at_ms
    }
    if (left.request_kind !== right.request_kind) {
      return left.request_kind === "tool" ? 1 : -1
    }
    return left.id.localeCompare(right.id)
  })
}

function countChildTools(llmTrace: TraceListItem, toolTraces: TraceListItem[]) {
  return toolTraces.filter((trace) => trace.parent_span_id === llmTrace.span_id)
    .length
}

export function buildTraceLoopGroups(
  loops: TraceLoopItem[],
  turns: TurnLifecycle[]
): TraceLoopGroup[] {
  const turnsById = new Map(turns.map((turn) => [turn.turn_id, turn]))

  return loops
    .map((loop) => {
      const ordered = sortByStartedAt(loop.traces)
      const llmTraces = ordered.filter((trace) => !isToolTrace(trace))
      const toolTraces = ordered.filter((trace) => isToolTrace(trace))
      const latestTrace = [...ordered].sort(
        (left, right) => right.started_at_ms - left.started_at_ms
      )[0]
      const latestLlmTrace = [...llmTraces].sort(
        (left, right) => right.started_at_ms - left.started_at_ms
      )[0]
      const turn = turnsById.get(loop.turn_id) ?? null
      const totalDurationMs = loop.duration_ms ?? 0
      const totalTokens = loop.total_tokens
      const finalStatus = loop.final_status
      const startedAtMs = loop.started_at_ms
      const finishedAtMs = loop.finished_at_ms

      const timeline: LoopTimelineNode[] = [
        {
          kind: "agent_root",
          id: loop.root_span_id || `${loop.trace_id}:root`,
          name: "invoke_agent aia.agent",
          operationName: "invoke_agent",
          spanKind: "INTERNAL",
          startedAtMs,
          finishedAtMs,
          durationMs:
            finishedAtMs != null
              ? Math.max(0, finishedAtMs - startedAtMs)
              : totalDurationMs,
          status: finalStatus,
          userMessage:
            turn?.user_message ??
            latestLlmTrace?.user_message ??
            latestTrace.user_message ??
            null,
        },
      ]

      for (const trace of ordered) {
        if (isToolTrace(trace)) {
          timeline.push({
            kind: "tool_span",
            id: trace.id,
            trace,
            name: toolSpanName(trace),
            operationName: "execute_tool",
            spanKind: "INTERNAL",
            startedAtMs: trace.started_at_ms,
            finishedAtMs: findFinishedAtMs(trace),
            durationMs: trace.duration_ms ?? 0,
            status: spanStatus(trace),
          })
          continue
        }

        timeline.push({
          kind: "llm_span",
          id: trace.id,
          trace,
          name: llmSpanName(trace),
          operationName: trace.operation_name,
          spanKind: trace.span_kind,
          startedAtMs: trace.started_at_ms,
          finishedAtMs: findFinishedAtMs(trace),
          durationMs: trace.duration_ms ?? 0,
          status: spanStatus(trace),
          toolCount: countChildTools(trace, toolTraces),
        })
      }

      return {
        key: loop.trace_id,
        requestKind: loop.request_kind,
        turnId: loop.turn_id,
        runId: loop.run_id,
        userMessage: turn?.user_message ?? loop.user_message,
        assistantMessage: turn?.assistant_message ?? null,
        model: loop.model,
        protocol: loop.protocol,
        endpointPath: loop.endpoint_path,
        latestStartedAtMs: loop.latest_started_at_ms,
        startedAtMs,
        finishedAtMs,
        totalDurationMs,
        totalTokens,
        stepCount: loop.llm_span_count,
        toolCount: loop.tool_span_count,
        failedToolCount: loop.failed_tool_count,
        finalStatus,
        traces: ordered,
        turn,
        pathSummary: ordered
          .map(summarizeStopReason)
          .filter((value): value is string => Boolean(value))
          .join(" -> "),
        latestError:
          loop.latest_error ?? turn?.failure_message ?? null,
        timeline,
        finalSpanId: loop.final_span_id,
      }
    })
    .sort((left, right) => right.latestStartedAtMs - left.latestStartedAtMs)
}
