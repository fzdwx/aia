import { MarkdownContent } from "@/components/markdown-content"
import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"

function getTapePressureSummary(details: Record<string, unknown> | undefined) {
  const pressureRatio = getNumberValue(details, "pressure_ratio")
  return pressureRatio != null
    ? `pressure ${(pressureRatio * 100).toFixed(1)}%`
    : "context usage"
}

export function createTapeInfoRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "TapeInfo",
    detailsPanelMode: "none",
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
    detailsPanelMode: "renderer-only-flat",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return getStringValue(args, "name") ?? "handoff"
    },
    renderSubtitle(data) {
      return truncateInline(
        getStringValue(data.arguments, "name") ?? data.outputContent,
        72
      )
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
            <section
              className="tool-timeline-detail-section tool-timeline-shell-detail"
              data-tool-detail-kind="content"
              data-tool-detail-tone="output"
            >
              <div className="tool-timeline-detail-body tool-timeline-shell-body">
                <MarkdownContent
                  className="text-body-sm leading-body-sm pr-10 text-pretty text-foreground/92"
                  content={summary}
                />
              </div>
            </section>
          ) : null}
        </>
      )
    },
  }
}
