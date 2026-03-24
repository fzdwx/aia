export function normalizeToolArguments(
  args: Record<string, unknown> | null | undefined
): Record<string, unknown> {
  if (!args || typeof args !== "object" || Array.isArray(args)) {
    return {}
  }
  return args
}

function getToolNameSegment(toolName: string | undefined): string | undefined {
  if (!toolName) return undefined
  const trimmed = toolName.trim()
  if (!trimmed) return undefined
  const segments = trimmed.split(".")
  return segments[segments.length - 1]
}

export function getToolDisplayName(toolName: string | undefined): string {
  return getToolNameSegment(toolName) ?? (toolName?.trim() || "tool")
}

function stringArg(
  args: Record<string, unknown>,
  ...keys: string[]
): string | undefined {
  for (const key of keys) {
    const value = args[key]
    if (typeof value === "string" && value.length > 0) {
      return value
    }
  }
  return undefined
}

export function getToolDisplayPath(
  toolName: string | undefined,
  details: Record<string, unknown> | undefined,
  args: Record<string, unknown> | null | undefined
): string {
  if (details) {
    if (typeof details.file_path === "string") return details.file_path
    if (typeof details.path === "string") return details.path
    if (typeof details.pattern === "string") return details.pattern
    if (typeof details.command === "string") return details.command
  }

  const normalizedArgs = normalizeToolArguments(args)
  const realToolName = getToolNameSegment(toolName)

  if (realToolName === "Glob") {
    return stringArg(normalizedArgs, "pattern", "path", "file_path") ?? ""
  }
  if (realToolName === "Grep") {
    return stringArg(normalizedArgs, "pattern", "path", "file_path") ?? ""
  }
  if (realToolName === "CodeSearch") {
    return stringArg(normalizedArgs, "query") ?? ""
  }
  if (realToolName === "WebSearch") {
    return stringArg(normalizedArgs, "query") ?? ""
  }
  if (realToolName === "Shell") {
    return stringArg(normalizedArgs, "command", "cmd") ?? ""
  }
  if (
    realToolName === "Read" ||
    realToolName === "Write" ||
    realToolName === "Edit"
  ) {
    return stringArg(normalizedArgs, "path", "file_path") ?? ""
  }

  const firstStr = Object.values(normalizedArgs).find(
    (v) => typeof v === "string"
  ) as string | undefined

  return firstStr ?? ""
}
