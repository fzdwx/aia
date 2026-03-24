import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

export function createTapeInfoRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "TapeInfo",
    renderTitle(data) {
      const pressureRatio = getNumberValue(data.details, "pressure_ratio")
      return pressureRatio != null
        ? `pressure ${(pressureRatio * 100).toFixed(1)}%`
        : "context usage"
    },
    renderMeta(data) {
      const totalEntries = getNumberValue(data.details, "total_entries")
      const anchorCount = getNumberValue(data.details, "anchor_count")
      const entriesSinceLastAnchor = getNumberValue(
        data.details,
        "entries_since_last_anchor"
      )

      if (
        totalEntries == null &&
        anchorCount == null &&
        entriesSinceLastAnchor == null
      ) {
        return null
      }

      return (
        <>
          {totalEntries != null
            ? createMetaBadge(
                `${totalEntries} entr${totalEntries === 1 ? "y" : "ies"}`
              )
            : null}
          {anchorCount != null
            ? createMetaBadge(
                `${anchorCount} anchor${anchorCount === 1 ? "" : "s"}`
              )
            : null}
          {entriesSinceLastAnchor != null
            ? createMetaBadge(`+${entriesSinceLastAnchor} since anchor`)
            : null}
        </>
      )
    },
    renderDetails() {
      return null
    },
  }
}

export function createTapeHandoffRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "TapeHandoff",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const summary = getStringValue(args, "summary")
      return [
        getStringValue(args, "name") ?? "handoff",
        summary ? truncateInline(summary, 72) : "",
      ]
        .filter(Boolean)
        .join(" — ")
    },
    renderDetails(data) {
      return data.outputContent ? (
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput
            value={data.outputContent}
            failed={!data.succeeded}
          />
        </ToolDetailSection>
      ) : null
    },
  }
}
