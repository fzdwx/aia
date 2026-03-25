export function normalizeToolArguments(
  args: Record<string, unknown> | null | undefined
): Record<string, unknown> {
  if (!args || typeof args !== "object" || Array.isArray(args)) {
    return {}
  }
  return args
}

let activeWorkspaceRoot: string | null = null

function normalizePathForDisplay(value: string): string {
  return value.trim().replace(/\\/g, "/").replace(/\/+$/, "")
}

export function setActiveWorkspaceRoot(
  workspaceRoot: string | null | undefined
): void {
  activeWorkspaceRoot =
    typeof workspaceRoot === "string" && workspaceRoot.trim().length > 0
      ? normalizePathForDisplay(workspaceRoot)
      : null
}

export function relativizeToActiveWorkspaceRoot(path: string): string {
  const normalizedPath = normalizePathForDisplay(path)
  if (!normalizedPath || !activeWorkspaceRoot) return normalizedPath

  if (normalizedPath === activeWorkspaceRoot) return ""

  const workspacePrefix = `${activeWorkspaceRoot}/`
  if (normalizedPath.startsWith(workspacePrefix)) {
    return normalizedPath.slice(workspacePrefix.length)
  }

  return normalizedPath
}

export function getFileDisplayParts(path: string): {
  fileName: string
  directory: string
} {
  const normalized = relativizeToActiveWorkspaceRoot(path)
  const segments = normalized.split("/").filter(Boolean)
  const fileName = segments[segments.length - 1] ?? normalized
  const directory = segments.slice(0, -1).join("/").replace(/^\.$/, "")

  return {
    fileName,
    directory,
  }
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
