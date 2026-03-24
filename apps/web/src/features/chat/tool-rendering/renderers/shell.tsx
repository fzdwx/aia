import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getStringValue, truncateInline } from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

export function createShellRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "shell",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const command = truncateInline(
        getStringValue(args, "command", "cmd") ??
          getToolDisplayPath(data.toolName, data.details, args),
        96
      )

      return command
        ? `${toolTimelineCopy.toolName.shell} — ${command}`
        : toolTimelineCopy.toolName.shell
    },
    renderDetails(data) {
      const content = data.succeeded
        ? (getStringValue(data.details, "stdout") ?? data.outputContent)
        : (getStringValue(data.details, "stderr") ?? data.outputContent)

      if (!content) return null

      return (
        <ToolDetailSection
          title={
            data.succeeded
              ? toolTimelineCopy.section.result
              : toolTimelineCopy.section.failure
          }
        >
          <ExpandableOutput value={content} failed={!data.succeeded} />
        </ToolDetailSection>
      )
    },
  }
}
