import { getToolDisplayName } from "@/lib/tool-display"
import type { TraceEvent } from "@/lib/types"
import {
  type LoopTimelineNode,
  type TraceLoopGroup,
} from "@/lib/trace-presentation"

import { asRecord, asString } from "@/lib/trace-inspection"

import { formatCount, truncate } from "./trace-panel-formatters"

export type TimelineTreeRow = {
  node: LoopTimelineNode
  depth: number
  hasChildren: boolean
}

export function findActiveNode(
  group: TraceLoopGroup | null,
  selectedNodeId: string | null
) {
  if (!group) return null
  if (selectedNodeId) {
    const matched = group.timeline.find((node) => node.id === selectedNodeId)
    if (matched) return matched
  }
  return group.timeline[group.timeline.length - 1] ?? null
}

export function nodeTitle(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return "invoke_agent"
    case "llm_span":
      return node.operationName
    case "tool_span":
      return getToolDisplayName(node.trace.model)
  }
}

export function nodeSubtitle(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return node.systemPromptPreview
        ? `system · ${truncate(node.systemPromptPreview, 120)}`
        : "root span"
    case "llm_span":
      return node.trace.total_tokens != null && node.trace.total_tokens > 0
        ? `${node.trace.model} · ${formatCount(node.trace.total_tokens)} tok`
        : node.trace.model
    case "tool_span":
      return node.trace.endpoint_path.startsWith("/tools/")
        ? node.operationName
        : node.trace.endpoint_path
  }
}

export function nodeKindLabel(node: LoopTimelineNode) {
  switch (node.kind) {
    case "agent_root":
      return "Agent"
    case "llm_span":
      return "LLM"
    case "tool_span":
      return "Tool"
  }
}

export function nodeSpanId(node: LoopTimelineNode) {
  return node.kind === "agent_root" ? node.id : node.trace.span_id
}

export function buildTimelineTreeRows(
  group: TraceLoopGroup
): TimelineTreeRow[] {
  if (group.timeline.length === 0) return []

  const spanIds = new Set(group.timeline.map((node) => nodeSpanId(node)))
  const root = group.timeline[0]
  const rootSpanId = nodeSpanId(root)
  const childrenByParent = new Map<string | null, LoopTimelineNode[]>()

  const pushChild = (parentId: string | null, node: LoopTimelineNode) => {
    const current = childrenByParent.get(parentId) ?? []
    current.push(node)
    childrenByParent.set(parentId, current)
  }

  for (const node of group.timeline) {
    if (node.kind === "agent_root") {
      pushChild(null, node)
      continue
    }

    const parentSpanId =
      node.trace.parent_span_id && spanIds.has(node.trace.parent_span_id)
        ? node.trace.parent_span_id
        : rootSpanId
    pushChild(parentSpanId, node)
  }

  for (const [, children] of childrenByParent) {
    children.sort((left, right) => {
      if (left.startedAtMs !== right.startedAtMs) {
        return left.startedAtMs - right.startedAtMs
      }
      return left.id.localeCompare(right.id)
    })
  }

  const rows: TimelineTreeRow[] = []
  const visit = (node: LoopTimelineNode, depth: number) => {
    const children = childrenByParent.get(nodeSpanId(node)) ?? []
    rows.push({
      node,
      depth,
      hasChildren: children.length > 0,
    })
    for (const child of children) {
      visit(child, depth + 1)
    }
  }

  for (const node of childrenByParent.get(null) ?? []) {
    visit(node, 0)
  }

  return rows
}

export function nodeTone(node: LoopTimelineNode) {
  if (node.kind === "agent_root") {
    return {
      frame: "trace-tone-agent-frame",
      dot: "trace-tone-agent-dot",
      badge: "trace-tone-agent-badge",
    }
  }

  if (node.kind === "tool_span") {
    return node.status === "error"
      ? {
          frame: "border-destructive/25 bg-destructive/[0.04]",
          dot: "border-destructive/30 bg-destructive/15 text-destructive",
          badge: "border-destructive/30 bg-destructive/[0.08] text-destructive",
        }
      : {
          frame: "trace-tone-tool-frame",
          dot: "trace-tone-tool-dot",
          badge: "trace-tone-tool-badge",
        }
  }

  return node.status === "error"
    ? {
        frame: "border-destructive/25 bg-destructive/[0.04]",
        dot: "border-destructive/30 bg-destructive/15 text-destructive",
        badge: "border-destructive/30 bg-destructive/[0.08] text-destructive",
      }
    : {
        frame: "border-border/40 bg-background/80",
        dot: "border-border/45 bg-muted/35 text-foreground/80",
        badge: "border-border/25 bg-muted/50 text-muted-foreground",
      }
}

export function summarizeEvent(event: TraceEvent) {
  const attributes = asEventRecord(event.attributes)
  switch (event.name) {
    case "response.first_text_delta":
    case "response.first_reasoning_delta":
      return asString(attributes?.preview) ?? null
    case "response.retrying": {
      const attempt = attributes?.attempt
      const maxAttempts = attributes?.max_attempts
      const reason = asString(attributes?.reason)
      const prefix =
        typeof attempt === "number" && typeof maxAttempts === "number"
          ? `attempt ${attempt + 1}/${maxAttempts}`
          : "retrying"
      return reason ? `${prefix} · ${reason}` : prefix
    }
    case "response.tool_call_detected":
    case "response.tool_call_started":
      return asString(attributes?.tool_name) ?? null
    case "response.completed":
      return asString(attributes?.stop_reason) ?? null
    case "response.failed":
      return asString(attributes?.error) ?? null
    default:
      return null
  }
}

export function buildRootEvents(group: TraceLoopGroup) {
  const events: Array<{
    key: string
    name: string
    at_ms: number
    summary?: string | null
    attributes?: Record<string, unknown> | null
  }> = [
    {
      key: `${group.key}:root:start`,
      name: "loop.started",
      at_ms: group.startedAtMs,
      summary: group.userMessage ? truncate(group.userMessage, 120) : null,
      attributes: {
        turn_id: group.turnId,
        run_id: group.runId,
      },
    },
  ]

  if (group.finishedAtMs != null) {
    events.push({
      key: `${group.key}:root:end`,
      name: group.finalStatus === "failed" ? "loop.failed" : "loop.completed",
      at_ms: group.finishedAtMs,
      summary: `${group.stepCount} llm spans · ${group.toolCount} tool spans`,
      attributes: {
        status: group.finalStatus,
        llm_spans: group.stepCount,
        tool_spans: group.toolCount,
        total_tokens: group.totalTokens,
      },
    })
  }

  return events
}

export function buildToolEvents(
  node: Extract<LoopTimelineNode, { kind: "tool_span" }>
) {
  const outcome =
    node.trace.status === "failed" ? "tool.failed" : "tool.completed"

  return [
    {
      key: `${node.id}:start`,
      name: "tool.started",
      at_ms: node.startedAtMs,
      summary: node.trace.model,
      attributes: {
        span_id: node.trace.span_id,
        tool_name: getToolDisplayName(node.trace.model),
      },
    },
    {
      key: `${node.id}:end`,
      name: outcome,
      at_ms: node.finishedAtMs ?? node.startedAtMs,
      summary: node.trace.error
        ? truncate(node.trace.error, 120)
        : (node.trace.stop_reason ?? null),
      attributes: {
        span_id: node.trace.span_id,
        tool_name: getToolDisplayName(node.trace.model),
        error: node.trace.error,
      },
    },
  ]
}

function asEventRecord(value: unknown): Record<string, unknown> | null {
  return asRecord(value)
}
