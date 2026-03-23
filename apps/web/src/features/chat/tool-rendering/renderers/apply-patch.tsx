import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

export function createApplyPatchRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "apply_patch",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const patch = getStringValue(args, "patch", "patchText")
      if (!patch) return getToolDisplayPath(data.toolName, data.details, args)
      const firstOperation = patch
        .split("\n")
        .find(
          (line) =>
            line.startsWith("*** Update File:") ||
            line.startsWith("*** Add File:") ||
            line.startsWith("*** Delete File:")
        )
      return truncateInline(firstOperation ?? "apply patch", 120)
    },
    renderMeta(data) {
      const linesAdded = getNumberValue(data.details, "lines_added")
      const linesRemoved = getNumberValue(data.details, "lines_removed")
      if (linesAdded == null && linesRemoved == null) return null
      return (
        <>
          {linesAdded != null
            ? createMetaBadge(`+${linesAdded}`, "text-foreground/70")
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
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput
            value={data.outputContent}
            failed={!data.succeeded}
          />
        </ToolDetailSection>
      )
    },
  }
}
