import { normalizeToolArguments } from "@/lib/tool-display"

import { truncateInline } from "./tool-rendering/helpers"

function quoteShellLike(value: string): string {
  return `"${value.replaceAll('"', '\\\"')}"`
}

export function formatSearchInvocation(
  _toolName: string,
  rawArguments: Record<string, unknown>,
  maxLength = 96
): string {
  const args = normalizeToolArguments(rawArguments)
  const pattern = typeof args.pattern === "string" ? args.pattern.trim() : ""
  const path = typeof args.path === "string" ? args.path.trim() : ""
  const glob = typeof args.glob === "string" ? args.glob.trim() : ""

  const parts: string[] = []
  if (pattern) parts.push(quoteShellLike(pattern))
  if (glob) parts.push(`--glob ${quoteShellLike(glob)}`)
  if (path) parts.push(path)

  return truncateInline(parts.join(" "), maxLength)
}
