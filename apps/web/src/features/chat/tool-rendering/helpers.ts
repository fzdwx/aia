import { createElement, type ReactNode } from "react"

export type DetailEntry = {
  label: string
  value: string | number
}

export function truncateInline(value: string, maxLength = 96): string {
  const compact = value.replace(/\s+/g, " ").trim()
  if (compact.length <= maxLength) return compact
  return `${compact.slice(0, maxLength - 1)}…`
}

export function formatScalar(value: unknown): string {
  if (typeof value === "string") return value
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value)
  }
  if (value == null) return "-"
  return JSON.stringify(value)
}

export function getStringValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): string | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === "string" && value.length > 0) {
      return value
    }
  }
  return undefined
}

export function getNumberValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): number | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === "number") {
      return value
    }
  }
  return undefined
}

export function getBooleanValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): boolean | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === "boolean") {
      return value
    }
  }
  return undefined
}

export function getArrayValue(
  record: Record<string, unknown> | undefined,
  key: string
): unknown[] {
  const value = record?.[key]
  return Array.isArray(value) ? value : []
}

export function createMetaBadge(
  content: ReactNode,
  className = "text-current"
): ReactNode {
  return createElement("span", { className: `shrink-0 ${className}` }, content)
}

export function compactPath(value: string, maxLength = 48): string {
  const compact = value.trim()
  if (compact.length <= maxLength) return compact

  const normalized = compact.replace(/\\/g, "/")
  const segments = normalized.split("/").filter(Boolean)
  if (segments.length <= 2) return truncateInline(compact, maxLength)

  for (let keep = segments.length; keep >= 2; keep -= 1) {
    const suffix = segments.slice(-keep).join("/")
    const candidate = `.../${suffix}`
    if (candidate.length <= maxLength) {
      return candidate
    }
  }

  return truncateInline(`.../${segments.slice(-1)[0] ?? compact}`, maxLength)
}

function toSentenceCase(key: string): string {
  const normalized = key.replace(/[_-]+/g, " ").trim()
  if (!normalized) return key

  return normalized.replace(/\b\w/g, (segment) => segment.toUpperCase())
}

function summarizeStructuredValue(value: unknown): string | number | null {
  if (typeof value === "string") {
    const compact = value.replace(/\s+/g, " ").trim()
    return compact ? truncateInline(compact, 84) : null
  }

  if (typeof value === "number") {
    return value
  }

  if (typeof value === "boolean") {
    return value ? "yes" : "no"
  }

  if (Array.isArray(value)) {
    return value.length > 0
      ? `${value.length} item${value.length === 1 ? "" : "s"}`
      : null
  }

  if (value && typeof value === "object") {
    const fieldCount = Object.keys(value).length
    return fieldCount > 0
      ? `${fieldCount} field${fieldCount === 1 ? "" : "s"}`
      : null
  }

  return null
}

export function buildDetailEntries(
  record: Record<string, unknown> | undefined,
  options?: {
    omitKeys?: Iterable<string>
    maxEntries?: number
  }
): DetailEntry[] {
  if (!record) return []

  const omitted = new Set(options?.omitKeys ?? [])
  const maxEntries = options?.maxEntries ?? 6

  return Object.entries(record)
    .filter(([key]) => !omitted.has(key))
    .map(([key, value]) => {
      const summarized = summarizeStructuredValue(value)
      if (summarized == null) return null

      return {
        label: toSentenceCase(key),
        value: summarized,
      }
    })
    .filter((entry): entry is DetailEntry => entry != null)
    .slice(0, maxEntries)
}
