import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput } from "../ui"

export function createApplyPatchRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "ApplyPatch",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const patch = getStringValue(args, "patch", "patchText")
      if (!patch) {
        const fallbackPath = getToolDisplayPath(
          data.toolName,
          data.details,
          args
        )
        return fallbackPath
          ? `${toolTimelineCopy.toolName.patch} — ${truncateInline(fallbackPath, 96)}`
          : toolTimelineCopy.toolName.patch
      }
      const firstOperation = patch
        .split("\n")
        .find(
          (line) =>
            line.startsWith("*** Update File:") ||
            line.startsWith("*** Add File:") ||
            line.startsWith("*** Delete File:")
        )
      const filePath = firstOperation?.split(":").slice(1).join(":").trim()

      return filePath
        ? `${toolTimelineCopy.toolName.patch} — ${truncateInline(filePath, 96)}`
        : toolTimelineCopy.toolName.patch
    },
    renderMeta(data) {
      const linesAdded = getNumberValue(data.details, "lines_added")
      const linesRemoved = getNumberValue(data.details, "lines_removed")
      if (linesAdded == null && linesRemoved == null) return null
      return (
        <>
          {linesAdded != null
            ? createMetaBadge(`+${linesAdded}`, "text-emerald-400")
            : null}
          {linesRemoved != null
            ? createMetaBadge(`-${linesRemoved}`, "text-red-400")
            : null}
        </>
      )
    },
    renderDetails(data) {
      if (!data.outputContent) return null

      return (
        <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
      )
    },
  }
}
