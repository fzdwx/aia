import type {
  StreamingToolOutput,
  ToolInvocationLifecycle,
  ToolOutputSegment,
} from "@/lib/types"

import { formatReadLineRange } from "./read-range"
import { formatSearchInvocation } from "./search-invocation"

export type ToolRowItem = {
  id: string
  toolName: string
  arguments: Record<string, unknown>
  detectedAtMs?: number
  startedAtMs?: number
  finishedAtMs?: number
  succeeded: boolean
  outputContent: string
  outputSegments?: ToolOutputSegment[]
  details?: Record<string, unknown>
}

type ToolCategory = "read" | "search" | "edit" | "other"

const CONTEXT_EXPLORATION_TOOLS = new Set([
  "Read",
  "Glob",
  "Grep",
  "list",
  "CodeSearch",
  "WebSearch",
])

const TOOL_CATEGORIES: Record<string, ToolCategory> = {
  Read: "read",
  cat: "read",
  head: "read",
  tail: "read",
  Grep: "search",
  CodeSearch: "search",
  WebSearch: "search",
  search: "search",
  find: "search",
  Glob: "search",
  ripgrep: "search",
  Shell: "other",
  Edit: "edit",
  Write: "edit",
  ApplyPatch: "edit",
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
  return TOOL_CATEGORIES[normalizeToolName(toolName)] ?? "other"
}

export function normalizeToolName(toolName: string): string {
  const trimmed = toolName.trim()
  if (!trimmed) return ""
  const segments = trimmed.split(".")
  return segments[segments.length - 1] ?? trimmed
}

export function isContextExplorationTool(toolName: string): boolean {
  return CONTEXT_EXPLORATION_TOOLS.has(normalizeToolName(toolName))
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
      detectedAtMs: inv.started_at_ms,
      arguments: call.arguments,
      startedAtMs: inv.started_at_ms,
      finishedAtMs: inv.finished_at_ms,
      succeeded: true,
      outputContent: outcome.result.content,
      outputSegments: undefined,
      details: outcome.result.details,
    }
  }
  return {
    id: call.invocation_id,
    toolName: call.tool_name,
    detectedAtMs: inv.started_at_ms,
    arguments: call.arguments,
    startedAtMs: inv.started_at_ms,
    finishedAtMs: inv.finished_at_ms,
    succeeded: false,
    outputContent: outcome.status === "failed" ? outcome.message : "",
    outputSegments: undefined,
  }
}

export function fromStreamingTool(tool: StreamingToolOutput): ToolRowItem {
  return {
    id: tool.invocationId,
    toolName: tool.toolName,
    arguments: tool.arguments,
    detectedAtMs: tool.detectedAtMs,
    startedAtMs: tool.startedAtMs,
    finishedAtMs: tool.finishedAtMs,
    succeeded: !tool.failed,
    outputContent: tool.resultContent ?? tool.output,
    outputSegments: tool.outputSegments,
    details: tool.resultDetails,
  }
}

function hasText(value: string | undefined): value is string {
  return typeof value === "string" && value.trim().length > 0
}

function mergeDetails(
  existing: Record<string, unknown> | undefined,
  incoming: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (!existing) return incoming
  if (!incoming) return existing
  return { ...existing, ...incoming }
}

function pickToolTimestamp(
  existing: number | undefined,
  incoming: number | undefined
): number {
  if (typeof existing === "number" && existing > 0) return existing
  if (typeof incoming === "number" && incoming > 0) return incoming
  return existing ?? incoming ?? 0
}

export function coalesceStreamingToolOutputs(
  toolOutputs: StreamingToolOutput[]
): StreamingToolOutput[] {
  const merged = new Map<string, StreamingToolOutput>()
  const order: string[] = []

  for (const tool of toolOutputs) {
    const key = tool.invocationId
    const existing = merged.get(key)

    if (!existing) {
      order.push(key)
      merged.set(key, {
        ...tool,
        arguments: { ...tool.arguments },
      })
      continue
    }

    merged.set(key, {
      invocationId: existing.invocationId,
      toolName: hasText(existing.toolName) ? existing.toolName : tool.toolName,
      arguments: { ...existing.arguments, ...tool.arguments },
      detectedAtMs: pickToolTimestamp(existing.detectedAtMs, tool.detectedAtMs),
      startedAtMs: existing.startedAtMs ?? tool.startedAtMs,
      finishedAtMs: existing.finishedAtMs ?? tool.finishedAtMs,
      output:
        existing.output.length >= tool.output.length
          ? existing.output
          : tool.output,
      outputSegments: [
        ...(existing.outputSegments ?? []),
        ...(tool.outputSegments ?? []),
      ],
      completed: existing.completed || tool.completed,
      resultContent: hasText(existing.resultContent)
        ? existing.resultContent
        : tool.resultContent,
      resultDetails: mergeDetails(existing.resultDetails, tool.resultDetails),
      failed: existing.failed ?? tool.failed,
    })
  }

  return order
    .map((key) => merged.get(key))
    .filter((tool): tool is StreamingToolOutput => tool != null)
}

export type ContextToolTriggerInfo = {
  title: string
  subtitle: string
  meta: string[]
  args: { key: string; value: string }[]
}

function buildReadContextMeta(item: ToolRowItem): string[] {
  const range = formatReadLineRange({
    offset: item.arguments.offset,
    limit: item.arguments.limit,
    linesRead: item.details?.lines_read,
    totalLines: item.details?.total_lines,
  })

  return range ? [range] : []
}

function buildSearchContextMeta(item: ToolRowItem): string[] {
  const matches =
    typeof item.details?.matches === "number" ? item.details.matches : null
  const returned =
    typeof item.details?.returned === "number" ? item.details.returned : null
  const truncated = item.details?.truncated === true

  if (matches == null) return []
  if (truncated && returned != null) {
    return [`${matches} matches`, `showing ${returned}`]
  }

  return [`${matches} matches`]
}

const CONTEXT_TRIGGER_ARG_OMIT = new Set([
  "offset",
  "limit",
  "glob",
  "content",
  "patch",
  "patchText",
  "old_string",
  "new_string",
  "value",
  "text",
  "input",
  "contents",
  "file_path",
  "path",
  "pattern",
  "command",
  "query",
])

export function contextToolTrigger(item: ToolRowItem): ContextToolTriggerInfo {
  const name = normalizeToolName(item.toolName)
  const args = item.arguments

  let title = name
  let subtitle = ""
  let meta: string[] = []
  const triggerArgs: { key: string; value: string }[] = []

  if (name === "Read") {
    subtitle =
      typeof args.file_path === "string"
        ? args.file_path
        : typeof args.path === "string"
          ? args.path
          : ""
    meta = buildReadContextMeta(item)
  } else if (name === "Grep") {
    subtitle = formatSearchInvocation(name, args)
    meta = buildSearchContextMeta(item)
  } else if (name === "Glob") {
    subtitle = formatSearchInvocation(name, args)
    meta = buildSearchContextMeta(item)
  } else if (name === "list") {
    subtitle = typeof args.path === "string" ? args.path : ""
  } else {
    const firstStr = Object.values(args).find((v) => typeof v === "string") as
      | string
      | undefined
    subtitle = firstStr ?? ""
  }

  for (const [key, val] of Object.entries(args)) {
    if (CONTEXT_TRIGGER_ARG_OMIT.has(key)) continue
    if (typeof val === "string" && val === subtitle) continue
    if (val == null) continue
    const display =
      typeof val === "string"
        ? val
        : typeof val === "number" || typeof val === "boolean"
          ? String(val)
          : null
    if (display != null) {
      triggerArgs.push({ key, value: display })
    }
  }

  return { title, subtitle, meta, args: triggerArgs }
}

export type ContextToolSummary = {
  read: number
  search: number
  list: number
}

export function contextToolSummary(items: ToolRowItem[]): ContextToolSummary {
  let read = 0
  let search = 0
  let list = 0

  for (const item of items) {
    const cat = categorize(item.toolName)
    if (cat === "read") read++
    else if (cat === "search") search++
    else list++
  }

  return { read, search, list }
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
