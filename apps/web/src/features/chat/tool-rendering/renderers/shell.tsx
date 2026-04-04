import type { ToolOutputSegment } from "@/lib/types"
import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getStringValue, truncateInline } from "../helpers"
import { stripAnsiSequences } from "./shell-output"
import { ShellOutputBody } from "./shell-output-body"

function buildShellOutput(data: {
  details?: Record<string, unknown>
  outputContent: string
  outputSegments?: ToolOutputSegment[]
}): string | null {
  if (data.outputContent.trim().length > 0) {
    return stripAnsiSequences(data.outputContent)
  }

  if ((data.outputSegments?.length ?? 0) > 0) {
    return (
      data.outputSegments
        ?.map((segment) => stripAnsiSequences(segment.text))
        .join("") ?? null
    )
  }

  const stdout = getStringValue(data.details, "stdout")
  const stderr = getStringValue(data.details, "stderr")
  const parts = [stdout, stderr].filter(
    (value): value is string => typeof value === "string" && value.length > 0
  )

  return parts.length > 0 ? stripAnsiSequences(parts.join("\n")) : null
}

function firstShellLine(data: {
  details?: Record<string, unknown>
  outputContent: string
  outputSegments?: ToolOutputSegment[]
}): string | null {
  const output = buildShellOutput(data)
  if (!output) return null

  const firstLine = output
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0)

  return firstLine ? truncateInline(firstLine, 96) : null
}

export function createShellRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Shell",
    detailsPanelMode: "renderer-only-flat",
    // Shell should only show duration after it has actually started,
    // not during the detected-but-not-started phase
    showDurationBeforeStart: false,
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
    renderSubtitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const description = getStringValue(args, "description")
      const command =
        getStringValue(data.details, "command") ??
        getStringValue(args, "command", "cmd")

      return (
        truncateInline(
          description ?? command ?? firstShellLine(data) ?? "",
          96
        ) || null
      )
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const command =
        getStringValue(data.details, "command") ??
        getStringValue(args, "command", "cmd")

      if (!command) return null

      const output = buildShellOutput(data)
      const segments = data.outputSegments ?? []

      return (
        <section
          className="tool-timeline-detail-section tool-timeline-shell-detail"
          data-tool-detail-kind="content"
          data-tool-detail-tone="output"
        >
          <div className="tool-timeline-detail-body tool-timeline-shell-body">
            <ShellOutputBody
              command={command}
              output={output}
              segments={segments}
              isRunning={data.isRunning ?? false}
            />
          </div>
        </section>
      )
    },
  }
}
