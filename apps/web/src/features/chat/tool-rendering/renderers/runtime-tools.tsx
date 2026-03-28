import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

function getTapePressureSummary(details: Record<string, unknown> | undefined) {
  const pressureRatio = getNumberValue(details, "pressure_ratio")
  return pressureRatio != null
    ? `pressure ${(pressureRatio * 100).toFixed(1)}%`
    : "context usage"
}

export function createTapeInfoRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "TapeInfo",
    renderTitle(data) {
      return getTapePressureSummary(data.details)
    },
    renderSubtitle(data) {
      return getTapePressureSummary(data.details)
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
      return getStringValue(args, "name") ?? "handoff"
    },
    renderSubtitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const summary = getStringValue(args, "summary")
      if (summary) return truncateInline(summary, 96)

      const name = getStringValue(args, "name")
      return name ? truncateInline(name, 72) : null
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const summary =
        getStringValue(args, "summary") ??
        getStringValue(data.details, "summary")
      const hasOutput = data.outputContent.trim().length > 0

      if (!summary && !hasOutput) return null

      return (
        <>
          {summary ? (
            <ToolDetailSection title="Summary">
              <p className="text-caption leading-body-sm text-pretty text-foreground/86">
                {summary}
              </p>
            </ToolDetailSection>
          ) : null}
          {hasOutput ? (
            <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
              <ExpandableOutput
                value={data.outputContent}
                failed={!data.succeeded}
              />
            </ToolDetailSection>
          ) : null}
        </>
      )
    },
  }
}
