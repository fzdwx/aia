import type { TraceListItem, TurnLifecycle } from "@/lib/types"

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
  traces: TraceListItem[],
  turns: TurnLifecycle[]
): TraceLoopGroup[] {
  const groups = new Map<string, TraceListItem[]>()
  const turnsById = new Map(turns.map((turn) => [turn.turn_id, turn]))

  for (const trace of traces) {
    const existing = groups.get(trace.trace_id)
    if (existing) {
      existing.push(trace)
    } else {
      groups.set(trace.trace_id, [trace])
    }
  }

  return Array.from(groups.entries())
    .map(([key, items]) => {
      const ordered = sortByStartedAt(items)
      const llmTraces = ordered.filter((trace) => !isToolTrace(trace))
      const toolTraces = ordered.filter((trace) => isToolTrace(trace))
      const latestTrace = [...ordered].sort(
        (left, right) => right.started_at_ms - left.started_at_ms
      )[0]
      const latestLlmTrace = [...llmTraces].sort(
        (left, right) => right.started_at_ms - left.started_at_ms
      )[0]
      const turn =
        turnsById.get((latestLlmTrace ?? latestTrace).turn_id) ?? null
      const totalDurationMs = ordered.reduce(
        (sum, trace) => sum + (trace.duration_ms ?? 0),
        0
      )
      const totalTokens = llmTraces.reduce(
        (sum, trace) => sum + (trace.total_tokens ?? 0),
        0
      )
      const finalStatus = loopStatus(llmTraces, toolTraces)
      const startedAtMs = Math.min(
        ...ordered.map((trace) => trace.started_at_ms)
      )
      const finishedCandidates = ordered
        .map(findFinishedAtMs)
        .filter((value): value is number => value != null)
      const turnFinishedAtMs = turn?.finished_at_ms ?? null
      const finishedAtMs =
        turnFinishedAtMs ??
        (finishedCandidates.length > 0 ? Math.max(...finishedCandidates) : null)

      const timeline: LoopTimelineNode[] = [
        {
          kind: "agent_root",
          id: ordered[0]?.root_span_id ?? `${key}:root`,
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
        key,
        turnId: (latestLlmTrace ?? latestTrace).turn_id,
        runId: (latestLlmTrace ?? latestTrace).run_id,
        userMessage:
          turn?.user_message ??
          latestLlmTrace?.user_message ??
          latestTrace.user_message ??
          null,
        assistantMessage: turn?.assistant_message ?? null,
        model: (latestLlmTrace ?? latestTrace).model,
        protocol: (latestLlmTrace ?? latestTrace).protocol,
        endpointPath: (latestLlmTrace ?? latestTrace).endpoint_path,
        latestStartedAtMs: latestTrace.started_at_ms,
        startedAtMs,
        finishedAtMs,
        totalDurationMs,
        totalTokens,
        stepCount: llmTraces.length,
        toolCount: toolTraces.length,
        failedToolCount: toolTraces.filter((trace) => trace.status === "failed")
          .length,
        finalStatus,
        traces: ordered,
        turn,
        pathSummary: ordered
          .map(summarizeStopReason)
          .filter((value): value is string => Boolean(value))
          .join(" -> "),
        latestError:
          [...ordered].reverse().find((trace) => trace.error)?.error ??
          turn?.failure_message ??
          null,
        timeline,
        finalSpanId: ordered[ordered.length - 1]?.id ?? null,
      }
    })
    .sort((left, right) => right.latestStartedAtMs - left.latestStartedAtMs)
}
