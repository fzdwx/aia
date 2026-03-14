export function normalizeToolArguments(
  args: Record<string, unknown> | null | undefined
): Record<string, unknown> {
  if (!args || typeof args !== "object" || Array.isArray(args)) {
    return {}
  }
  return args
}

export function getToolDisplayPath(
  details: Record<string, unknown> | undefined,
  args: Record<string, unknown> | null | undefined
): string {
  if (details) {
    if (typeof details.file_path === "string") return details.file_path
    if (typeof details.pattern === "string") return details.pattern
    if (typeof details.command === "string") return details.command
  }

  const normalizedArgs = normalizeToolArguments(args)
  const firstStr = Object.values(normalizedArgs).find(
    (v) => typeof v === "string"
  ) as string | undefined

  return firstStr ?? ""
}
