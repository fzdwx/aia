export function normalizeToolArguments(
  args: Record<string, unknown> | null | undefined
): Record<string, unknown> {
  if (!args || typeof args !== "object" || Array.isArray(args)) {
    return {}
  }
  return args
}

function stringArg(
  args: Record<string, unknown>,
  key: string
): string | undefined {
  const value = args[key]
  return typeof value === "string" && value.length > 0 ? value : undefined
}

export function getToolDisplayPath(
  toolName: string | undefined,
  details: Record<string, unknown> | undefined,
  args: Record<string, unknown> | null | undefined
): string {
  if (details) {
    if (typeof details.file_path === "string") return details.file_path
    if (typeof details.pattern === "string") return details.pattern
    if (typeof details.command === "string") return details.command
  }

  const normalizedArgs = normalizeToolArguments(args)
  const normalizedToolName = toolName?.toLowerCase()

  if (normalizedToolName === "glob") {
    return stringArg(normalizedArgs, "pattern") ?? stringArg(normalizedArgs, "path") ?? ""
  }
  if (normalizedToolName === "grep") {
    return stringArg(normalizedArgs, "pattern") ?? stringArg(normalizedArgs, "path") ?? ""
  }
  if (normalizedToolName === "shell") {
    return stringArg(normalizedArgs, "command") ?? ""
  }
  if (
    normalizedToolName === "read" ||
    normalizedToolName === "write" ||
    normalizedToolName === "edit"
  ) {
    return stringArg(normalizedArgs, "path") ?? ""
  }

  const firstStr = Object.values(normalizedArgs).find(
    (v) => typeof v === "string"
  ) as string | undefined

  return firstStr ?? ""
}
