import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getStringValue, truncateInline } from "../helpers"

function buildShellOutput(data: {
  details?: Record<string, unknown>
  outputContent: string
}): string | null {
  if (data.outputContent.trim().length > 0) {
    return data.outputContent
  }

  const stdout = getStringValue(data.details, "stdout")
  const stderr = getStringValue(data.details, "stderr")
  const parts = [stdout, stderr].filter(
    (value): value is string => typeof value === "string" && value.length > 0
  )

  return parts.length > 0 ? parts.join("\n") : null
}

export function createShellRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Shell",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const descriptionValue = getStringValue(args, "description")
      const description = descriptionValue
        ? truncateInline(descriptionValue, 96)
        : null
      const command = truncateInline(
        getStringValue(args, "command", "cmd") ??
          getToolDisplayPath(data.toolName, data.details, args),
        96
      )

      return description || command || toolTimelineCopy.toolName.shell
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const command =
        getStringValue(data.details, "command") ??
        getStringValue(args, "command", "cmd")

      if (!command) return null

      const output = buildShellOutput(data)
      const value = output ? `$ ${command}\n\n${output}` : `$ ${command}`

      return (
        <section
          className="tool-timeline-detail-section tool-timeline-shell-detail"
          data-tool-detail-kind="content"
          data-tool-detail-tone="output"
        >
          <div className="tool-timeline-detail-body tool-timeline-shell-body">
            <pre className="tool-timeline-shell-pre">{value}</pre>
          </div>
        </section>
      )
    },
  }
}
