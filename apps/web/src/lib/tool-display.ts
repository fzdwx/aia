export function normalizeToolArguments(
  args: Record<string, unknown> | null | undefined
): Record<string, unknown> {
  if (!args || typeof args !== "object" || Array.isArray(args)) {
    return {}
  }
  return args
}

function normalizeToolName(toolName: string | undefined): string | undefined {
  if (!toolName) return undefined
  const lower = toolName.toLowerCase()
  const segments = lower.split(".")
  return segments[segments.length - 1]
}

export function getToolDisplayName(toolName: string | undefined): string {
  return normalizeToolName(toolName) ?? (toolName?.trim() || "tool")
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
  const normalizedToolName = normalizeToolName(toolName)

  if (normalizedToolName === "glob") {
    return stringArg(normalizedArgs, "pattern", "path", "file_path") ?? ""
  }
  if (normalizedToolName === "grep") {
    return stringArg(normalizedArgs, "pattern", "path", "file_path") ?? ""
  }
  if (normalizedToolName === "codesearch") {
    return stringArg(normalizedArgs, "query") ?? ""
  }
  if (normalizedToolName === "websearch") {
    return stringArg(normalizedArgs, "query") ?? ""
  }
  if (normalizedToolName === "shell") {
    return stringArg(normalizedArgs, "command", "cmd") ?? ""
  }
  if (
    normalizedToolName === "read" ||
    normalizedToolName === "write" ||
    normalizedToolName === "edit"
  ) {
    return stringArg(normalizedArgs, "path", "file_path") ?? ""
  }

  const firstStr = Object.values(normalizedArgs).find(
    (v) => typeof v === "string"
  ) as string | undefined

  return firstStr ?? ""
}
