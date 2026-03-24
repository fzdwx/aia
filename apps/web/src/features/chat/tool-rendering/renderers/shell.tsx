import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getStringValue, truncateInline } from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

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
      const stdout = getStringValue(data.details, "stdout")
      const stderr = getStringValue(data.details, "stderr")
      const fallbackContent =
        !stdout && !stderr && data.outputContent ? data.outputContent : null

      if (!command && !stdout && !stderr && !fallbackContent) return null

      return (
        <>
          {command ? (
            <section
              className="tool-timeline-detail-section"
              data-tool-detail-kind="content"
              data-tool-detail-tone="output"
            >
              <div className="tool-timeline-detail-body">
                <ExpandableOutput value={`$ ${command}`} failed={false} />
              </div>
            </section>
          ) : null}
          {stdout ? (
            <ToolDetailSection title={toolTimelineCopy.section.result}>
              <ExpandableOutput value={stdout} failed={false} />
            </ToolDetailSection>
          ) : null}
          {stderr ? (
            <ToolDetailSection title={toolTimelineCopy.section.failure}>
              <ExpandableOutput value={stderr} failed />
            </ToolDetailSection>
          ) : null}
          {fallbackContent ? (
            <ToolDetailSection
              title={
                data.succeeded
                  ? toolTimelineCopy.section.result
                  : toolTimelineCopy.section.failure
              }
            >
              <ExpandableOutput
                value={fallbackContent}
                failed={!data.succeeded}
              />
            </ToolDetailSection>
          ) : null}
        </>
      )
    },
  }
}
