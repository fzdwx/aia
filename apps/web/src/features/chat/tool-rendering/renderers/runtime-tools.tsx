import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import { getNumberValue, getStringValue, truncateInline } from "../helpers"
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
