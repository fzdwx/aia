import type { StreamingToolOutput, ToolInvocationLifecycle } from "@/lib/types"

export type ToolRowItem = {
  id: string
  toolName: string
  arguments: Record<string, unknown>
  startedAtMs?: number
  finishedAtMs?: number
  succeeded: boolean
  outputContent: string
  details?: Record<string, unknown>
}

type ToolCategory = "read" | "search" | "edit" | "other"

const TOOL_CATEGORIES: Record<string, ToolCategory> = {
  read: "read",
  cat: "read",
  head: "read",
  tail: "read",
  grep: "search",
  codesearch: "search",
  websearch: "search",
  search: "search",
  find: "search",
  glob: "search",
  ripgrep: "search",
  shell: "other",
  edit: "edit",
  write: "edit",
  apply_patch: "edit",
  replace: "edit",
  sed: "edit",
}

const CATEGORY_LABELS: Record<ToolCategory, string> = {
  read: "read",
  search: "search",
  edit: "edit",
  other: "tool",
}

function categorize(toolName: string): ToolCategory {
  return TOOL_CATEGORIES[toolName.toLowerCase()] ?? "other"
}

export function buildCategorySummary(
  invocations: { toolName: string }[]
): { category: ToolCategory; label: string; count: number }[] {
  const counts = new Map<ToolCategory, number>()
  for (const inv of invocations) {
    const cat = categorize(inv.toolName)
    counts.set(cat, (counts.get(cat) ?? 0) + 1)
  }
  return Array.from(counts.entries()).map(([cat, count]) => ({
    category: cat,
    label: CATEGORY_LABELS[cat],
    count,
  }))
}

export function fromInvocation(inv: ToolInvocationLifecycle): ToolRowItem {
  const { call, outcome } = inv
  if (outcome.status === "succeeded") {
    return {
      id: call.invocation_id,
      toolName: call.tool_name,
      arguments: call.arguments,
      startedAtMs: inv.started_at_ms,
      finishedAtMs: inv.finished_at_ms,
      succeeded: true,
      outputContent: outcome.result.content,
      details: outcome.result.details,
    }
  }
  return {
    id: call.invocation_id,
    toolName: call.tool_name,
    arguments: call.arguments,
    startedAtMs: inv.started_at_ms,
    finishedAtMs: inv.finished_at_ms,
    succeeded: false,
    outputContent: outcome.status === "failed" ? outcome.message : "",
  }
}

export function fromStreamingTool(tool: StreamingToolOutput): ToolRowItem {
  return {
    id: tool.invocationId,
    toolName: tool.toolName,
    arguments: tool.arguments,
    startedAtMs: tool.startedAtMs ?? tool.detectedAtMs,
    finishedAtMs: tool.finishedAtMs,
    succeeded: !tool.failed,
    outputContent: tool.resultContent ?? tool.output,
    details: tool.resultDetails,
  }
}

export function formatDurationMs(
  startedAtMs: number | undefined,
  finishedAtMs?: number,
  options?: { live?: boolean }
): string | null {
  if (!startedAtMs) return null
  const end = finishedAtMs ?? Date.now()
  const duration = Math.max(0, end - startedAtMs)

  if (options?.live && finishedAtMs == null && duration < 60_000) {
    const seconds = Math.floor(duration / 100) / 10
    return `${seconds.toFixed(1)} s`
  }

  if (duration < 1000) return `${duration} ms`
  if (duration < 60_000) return `${(duration / 1000).toFixed(1)} s`
  const minutes = Math.floor(duration / 60_000)
  const seconds = Math.floor((duration % 60_000) / 1000)
  return `${minutes}m ${seconds}s`
}
