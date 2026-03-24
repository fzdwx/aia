type ReadRangeInput = {
  offset?: unknown
  limit?: unknown
  linesRead?: unknown
  totalLines?: unknown
}

function getNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null
}

export function formatReadLineRange({
  offset,
  limit,
  linesRead,
  totalLines,
}: ReadRangeInput): string | null {
  const normalizedOffset = Math.max(0, getNumber(offset) ?? 0)
  const normalizedLimit = getNumber(limit)
  const normalizedLinesRead = getNumber(linesRead)
  const normalizedTotalLines = getNumber(totalLines)

  if (normalizedLinesRead != null) {
    if (normalizedLinesRead <= 0) return "0L"
    return `L${normalizedOffset + 1}~${normalizedOffset + normalizedLinesRead}`
  }

  if (normalizedTotalLines != null && normalizedTotalLines > 0) {
    if (normalizedOffset === 0) {
      return `L1~${normalizedTotalLines}`
    }

    const fallbackUpperBound =
      normalizedLimit != null && normalizedLimit > 0
        ? normalizedOffset + normalizedLimit
        : normalizedTotalLines
    const endLine = Math.min(normalizedTotalLines, fallbackUpperBound)

    if (endLine <= normalizedOffset) return "0L"
    return `L${normalizedOffset + 1}~${endLine}`
  }

  if (normalizedLimit != null) {
    if (normalizedLimit <= 0) return "0L"
    return `L${normalizedOffset + 1}~${normalizedOffset + normalizedLimit}`
  }

  return null
}
