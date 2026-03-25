import { getToolDisplayName } from "@/lib/tool-display"
import type { TraceDashboardRange } from "@/lib/types"
import {
  formatTraceDuration,
  type TraceLoopGroup,
} from "@/lib/trace-presentation"

export function formatOverviewRangeLabel(range: TraceDashboardRange) {
  switch (range) {
    case "today":
      return "today"
    case "week":
      return "the last 7 days"
    case "month":
      return "the last 30 days"
  }
}

export function formatDateTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  })
}

export function formatCount(value: number | null | undefined) {
  return value != null ? value.toLocaleString("en-US") : "-"
}

export function truncate(text: string, max: number) {
  if (text.length <= max) return text
  return `${text.slice(0, max - 1)}...`
}

export function compactId(value: string, head = 8, tail = 6) {
  if (value.length <= head + tail + 1) return value
  return `${value.slice(0, head)}...${value.slice(-tail)}`
}

export function loopBadgeVariant(status: TraceLoopGroup["finalStatus"]) {
  switch (status) {
    case "failed":
      return "destructive" as const
    case "partial":
      return "outline" as const
    default:
      return "secondary" as const
  }
}

export function loopWindowMs(group: TraceLoopGroup) {
  const explicit =
    group.finishedAtMs != null ? group.finishedAtMs - group.startedAtMs : null
  return Math.max(1, explicit ?? group.totalDurationMs ?? 1)
}

export function relativeOffsetLabel(
  nodeStartedAtMs: number,
  groupStartedAtMs: number
) {
  const delta = Math.max(0, nodeStartedAtMs - groupStartedAtMs)
  return `+${formatTraceDuration(delta)}`
}

export function formatToolName(name: string) {
  return getToolDisplayName(name)
}
