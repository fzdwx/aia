export type JsonRecord = Record<string, unknown>

export function isJsonRecord(value: unknown): value is JsonRecord {
  return value != null && typeof value === "object" && !Array.isArray(value)
}

export function asRecord(value: unknown): JsonRecord | null {
  return isJsonRecord(value) ? value : null
}

export function asString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null
}

export function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

export function extractTraceText(value: unknown): string {
  if (typeof value === "string") return value

  if (Array.isArray(value)) {
    return value
      .map((item) => extractTraceText(item))
      .filter(Boolean)
      .join("\n")
      .trim()
  }

  const record = asRecord(value)
  if (!record) return ""

  for (const key of ["text", "summary_text", "content", "output", "value"]) {
    const text = extractTraceText(record[key])
    if (text) return text
  }

  return Object.values(record)
    .map((item) => extractTraceText(item))
    .filter(Boolean)
    .join(" ")
    .trim()
}
