import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getStringValue, truncateInline } from "../helpers"
import { ExpandableOutput } from "../ui"

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

      return (
        <section
          className="tool-timeline-detail-section"
          data-tool-detail-kind="content"
          data-tool-detail-tone="output"
        >
          <div className="tool-timeline-detail-body">
            <ExpandableOutput value={`$ ${command}`} failed={false} />
          </div>
        </section>
      )
    },
  }
}
