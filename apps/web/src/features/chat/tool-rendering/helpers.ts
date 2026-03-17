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
