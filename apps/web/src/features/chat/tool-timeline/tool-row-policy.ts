import type { ToolRowItem } from "@/features/chat/tool-timeline-helpers"

import { normalizeToolName } from "@/features/chat/tool-timeline-helpers"

const EMPTY_TOOL_RESULT_FALLBACK = "No output returned."
const FAILED_TOOL_RESULT_FALLBACK = "Tool execution failed."
const EMPTY_QUESTION_RESULT_FALLBACK = "No additional details."
const IGNORED_QUESTION_RESULT_FALLBACK = "Question ignored."
const INLINE_DETAIL_TOOLS = new Set<string>()

function hasText(value: string | undefined | null): value is string {
  return typeof value === "string" && value.trim().length > 0
}

function firstNonEmptyLine(value: string): string | null {
  const line = value
    .split("\n")
    .map((entry) => entry.trim())
    .find((entry) => entry.length > 0)

  return line ?? null
}

function toolFirstVisibleLine(item: ToolRowItem): string | null {
  return firstNonEmptyLine(item.outputContent)
}

function hasMeaningfulValue(value: unknown): boolean {
  if (value == null) return false
  if (typeof value === "string") return value.trim().length > 0
  if (Array.isArray(value)) {
    return value.some((entry) => hasMeaningfulValue(entry))
  }
  if (typeof value === "object") {
    return Object.values(value as Record<string, unknown>).some((entry) =>
      hasMeaningfulValue(entry)
    )
  }

  return true
}

function getStringRecordValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): string | null {
  if (!record) return null

  for (const key of keys) {
    const value = record[key]
    if (typeof value === "string" && value.trim().length > 0) {
      return value
    }
  }

  return null
}

function getBooleanRecordValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): boolean {
  if (!record) return false

  return keys.some((key) => record[key] === true)
}

export function isIgnoredQuestion(item: ToolRowItem): boolean {
  if (normalizeToolName(item.toolName) !== "question") return false

  const status = getStringRecordValue(item.details, "status", "state")
  const action = getStringRecordValue(item.details, "action")

  return (
    getBooleanRecordValue(
      item.details,
      "ignored",
      "was_ignored",
      "skip_render"
    ) ||
    getBooleanRecordValue(item.arguments, "ignored") ||
    status?.toLowerCase() === "ignored" ||
    action?.toLowerCase() === "ignore" ||
    action?.toLowerCase() === "ignored" ||
    item.outputContent.toLowerCase().includes("ignored")
  )
}

export function hasQuestionResolution(item: ToolRowItem): boolean {
  if (hasText(item.outputContent)) return true

  return hasMeaningfulValue({
    summary: getStringRecordValue(item.details, "summary"),
    answer: getStringRecordValue(item.details, "answer"),
    reason: getStringRecordValue(item.details, "reason"),
    message: getStringRecordValue(item.details, "message"),
  })
}

export function shouldRenderToolItem(item: ToolRowItem): boolean {
  if (normalizeToolName(item.toolName) !== "question") return true
  if (!item.succeeded) return true
  if (isIgnoredQuestion(item)) return true

  return hasQuestionResolution(item)
}

export function hasVisibleToolDetails(item: ToolRowItem): boolean {
  return hasText(item.outputContent) || hasMeaningfulValue(item.details)
}

export function getFallbackSubtitle(item: ToolRowItem): string | null {
  if (normalizeToolName(item.toolName) === "question") {
    if (isIgnoredQuestion(item)) return IGNORED_QUESTION_RESULT_FALLBACK
    if (!item.succeeded) {
      return hasText(item.outputContent)
        ? item.outputContent.trim()
        : FAILED_TOOL_RESULT_FALLBACK
    }
    if (item.finishedAtMs != null && !hasQuestionResolution(item)) {
      return EMPTY_QUESTION_RESULT_FALLBACK
    }

    return null
  }

  if (!item.succeeded) {
    return hasText(item.outputContent)
      ? item.outputContent.trim()
      : FAILED_TOOL_RESULT_FALLBACK
  }

  if (toolFirstVisibleLine(item)) {
    return toolFirstVisibleLine(item)
  }

  if (item.finishedAtMs != null && !hasVisibleToolDetails(item)) {
    return EMPTY_TOOL_RESULT_FALLBACK
  }

  return null
}

export function shouldInlineToolDetails(item: ToolRowItem) {
  return INLINE_DETAIL_TOOLS.has(normalizeToolName(item.toolName))
}

export function shouldShowToolRowCaret() {
  return false
}
